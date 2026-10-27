use crate::model::{PolyhedronFileType, PolyhedronPrimitive};
use std::collections::HashMap;
use std::fmt;
use std::fs;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolyhedronReaderError {
    UnsupportedFileType,
    Io { path: String, message: String },
    InvalidFile(String),
}

impl fmt::Display for PolyhedronReaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFileType => formatter.write_str("unsupported polyhedron file type"),
            Self::Io { path, message } => {
                write!(
                    formatter,
                    "failed to read polyhedron file {path:?}: {message}"
                )
            }
            Self::InvalidFile(message) => {
                write!(formatter, "invalid polyhedron file: {message}")
            }
        }
    }
}

impl std::error::Error for PolyhedronReaderError {}

pub(crate) fn read_file(
    file_type: PolyhedronFileType,
    filename: &str,
) -> Result<PolyhedronPrimitive, PolyhedronReaderError> {
    let data = fs::read(filename).map_err(|error| PolyhedronReaderError::Io {
        path: filename.to_owned(),
        message: error.to_string(),
    })?;
    let shape = match file_type {
        PolyhedronFileType::Stl => read_stl(&data),
        PolyhedronFileType::Ply => read_ply(&data),
        PolyhedronFileType::Unknown => Err(PolyhedronReaderError::UnsupportedFileType),
    }?;
    // Native CSPrimPolyhedronReader::ReadFile():
    //   (points == 0) || (cells == 0)  ->  "file invalid or empty, skipping".
    if shape.vertex_count() == 0 || shape.face_count() == 0 {
        return Err(invalid_file(
            "file invalid or empty (no points or no cells)",
        ));
    }
    Ok(shape)
}

fn read_stl(data: &[u8]) -> Result<PolyhedronPrimitive, PolyhedronReaderError> {
    let binary = read_binary_stl(data);
    if let Some(text) = std::str::from_utf8(data).ok().filter(|text| {
        text.trim_start()
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().next())
            .is_some_and(|token| token.eq_ignore_ascii_case("solid"))
    }) {
        return match read_ascii_stl(text) {
            Ok(shape) => Ok(shape),
            Err(ascii_error) if has_binary_stl_payload(data) => binary.or(Err(ascii_error)),
            Err(ascii_error) => Err(ascii_error),
        };
    }
    binary
}

fn has_binary_stl_payload(data: &[u8]) -> bool {
    data.get(84..).is_some_and(|payload| {
        payload
            .iter()
            .any(|byte| byte.is_ascii_control() && !matches!(*byte, b'\t' | b'\n' | b'\r'))
    })
}

fn read_ascii_stl(text: &str) -> Result<PolyhedronPrimitive, PolyhedronReaderError> {
    #[derive(Clone, Copy)]
    enum State {
        AwaitSolid,
        InSolid,
        AwaitOuterLoop,
        ReadingVertices(usize),
        AwaitEndLoop,
        AwaitEndFacet,
        BetweenSolids,
    }

    let mut vertices = Vec::new();
    let mut state = State::AwaitSolid;
    let mut found_solid = false;
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        let Some(token) = tokens.next() else {
            continue;
        };
        state = match state {
            State::AwaitSolid | State::BetweenSolids => {
                if !token.eq_ignore_ascii_case("solid") {
                    return Err(invalid_file("ASCII STL must start each solid with `solid`"));
                }
                found_solid = true;
                State::InSolid
            }
            State::InSolid if token.eq_ignore_ascii_case("facet") => State::AwaitOuterLoop,
            State::InSolid if token.eq_ignore_ascii_case("endsolid") => State::BetweenSolids,
            State::InSolid => {
                return Err(invalid_file(format!(
                    "expected `facet` or `endsolid`, found {token:?}"
                )));
            }
            State::AwaitOuterLoop => {
                if !token.eq_ignore_ascii_case("outer")
                    || !tokens
                        .next()
                        .is_some_and(|token| token.eq_ignore_ascii_case("loop"))
                {
                    return Err(invalid_file("invalid ASCII STL outer loop"));
                }
                State::ReadingVertices(0)
            }
            State::ReadingVertices(count) => {
                if !token.eq_ignore_ascii_case("vertex") {
                    return Err(invalid_file("ASCII STL facet is missing a vertex"));
                }
                let mut coordinates = [0.0; 3];
                for coordinate in &mut coordinates {
                    let value = tokens
                        .next()
                        .ok_or_else(|| invalid_file("STL vertex is missing a coordinate"))?;
                    *coordinate = parse_finite_float(value, "STL vertex coordinate")?;
                }
                vertices.push(coordinates);
                if count == 2 {
                    State::AwaitEndLoop
                } else {
                    State::ReadingVertices(count + 1)
                }
            }
            State::AwaitEndLoop => {
                if !token.eq_ignore_ascii_case("endloop") {
                    return Err(invalid_file("ASCII STL facet is missing `endloop`"));
                }
                State::AwaitEndFacet
            }
            State::AwaitEndFacet => {
                if !token.eq_ignore_ascii_case("endfacet") {
                    return Err(invalid_file("ASCII STL facet is missing `endfacet`"));
                }
                State::InSolid
            }
        }
    }
    if !found_solid || !matches!(state, State::BetweenSolids) {
        return Err(invalid_file("ASCII STL solid or facet is incomplete"));
    }
    if vertices.is_empty() {
        return Err(invalid_file(
            "ASCII STL must contain one or more complete triangular facets",
        ));
    }
    shape_from_triangles(vertices)
}

fn read_binary_stl(data: &[u8]) -> Result<PolyhedronPrimitive, PolyhedronReaderError> {
    if data.len() < 84 {
        return Err(invalid_file("binary STL header is truncated"));
    }
    let triangle_count = (data.len() - 84) / 50;
    if triangle_count == 0 {
        return Err(invalid_file("binary STL contains no facets"));
    }

    let mut vertices = Vec::with_capacity(triangle_count * 3);
    for triangle in 0..triangle_count {
        let facet_start = 84 + triangle * 50 + 12;
        for vertex in 0..3 {
            let start = facet_start + vertex * 12;
            vertices.push([
                read_f32_le(&data[start..start + 4]) as f64,
                read_f32_le(&data[start + 4..start + 8]) as f64,
                read_f32_le(&data[start + 8..start + 12]) as f64,
            ]);
        }
    }
    if vertices
        .iter()
        .flatten()
        .any(|coordinate| !coordinate.is_finite())
    {
        return Err(invalid_file("binary STL contains a non-finite coordinate"));
    }
    shape_from_triangles(vertices)
}

fn shape_from_triangles(
    vertices: Vec<[f64; 3]>,
) -> Result<PolyhedronPrimitive, PolyhedronReaderError> {
    let mut shape = PolyhedronPrimitive::default();
    let mut points = HashMap::new();
    let (triangles, remainder) = vertices.as_chunks::<3>();
    if !remainder.is_empty() {
        return Err(invalid_file("STL facet vertices are incomplete"));
    }
    for triangle in triangles {
        let mut face = Vec::with_capacity(3);
        for coordinates in triangle {
            let key = coordinates.map(|coordinate| {
                let coordinate = coordinate as f32;
                if coordinate == 0.0 {
                    0.0f32.to_bits()
                } else {
                    coordinate.to_bits()
                }
            });
            let index = if let Some(index) = points.get(&key) {
                *index
            } else {
                let index = i32::try_from(shape.add_vertex(*coordinates))
                    .map_err(|_| invalid_file("STL contains too many vertices"))?;
                points.insert(key, index);
                index
            };
            face.push(index);
        }
        shape.add_face(face);
    }
    Ok(shape)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlyFormat {
    Ascii,
    BinaryLittleEndian,
    BinaryBigEndian,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlyScalarType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl PlyScalarType {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "char" | "int8" => Some(Self::I8),
            "uchar" | "uint8" => Some(Self::U8),
            "short" | "int16" => Some(Self::I16),
            "ushort" | "uint16" => Some(Self::U16),
            "int" | "int32" => Some(Self::I32),
            "uint" | "uint32" => Some(Self::U32),
            "float" | "float32" => Some(Self::F32),
            "double" | "float64" => Some(Self::F64),
            _ => None,
        }
    }

    fn is_integer(self) -> bool {
        !matches!(self, Self::F32 | Self::F64)
    }

    fn byte_width(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

#[derive(Clone, Debug)]
enum PlyProperty {
    Scalar {
        name: String,
        data_type: PlyScalarType,
    },
    List {
        name: String,
        count_type: PlyScalarType,
        item_type: PlyScalarType,
    },
}

#[derive(Clone, Debug)]
struct PlyElement {
    name: String,
    count: usize,
    properties: Vec<PlyProperty>,
}

fn read_ply(data: &[u8]) -> Result<PolyhedronPrimitive, PolyhedronReaderError> {
    let (format, elements, body_start) = parse_ply_header(data)?;
    let mut body = PlyBody {
        data: &data[body_start..],
        offset: 0,
        ascii_tokens: None,
    };
    if format == PlyFormat::Ascii {
        body.ascii_tokens = Some(
            std::str::from_utf8(&data[body_start..])
                .map_err(|_| invalid_file("ASCII PLY body is not UTF-8"))?
                .split_whitespace()
                .collect(),
        );
    }

    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    for element in elements {
        if element.properties.is_empty() {
            continue;
        }
        if element.count > body.data.len() {
            return Err(invalid_file(
                "PLY element count exceeds the available file data",
            ));
        }
        for _ in 0..element.count {
            let mut coordinates = [None; 3];
            let mut face_indices = None;
            for property in &element.properties {
                match property {
                    PlyProperty::Scalar { name, data_type } => {
                        let value = body.read_value(format, *data_type)?;
                        if element.name == "vertex" {
                            match name.as_str() {
                                "x" => coordinates[0] = Some(value),
                                "y" => coordinates[1] = Some(value),
                                "z" => coordinates[2] = Some(value),
                                _ => {}
                            }
                        }
                    }
                    PlyProperty::List {
                        name,
                        count_type,
                        item_type,
                    } => {
                        let count_value = body.read_value(format, *count_type)?;
                        let count = nonnegative_integer(count_value, "PLY list count")?;
                        let is_face_indices = element.name == "face"
                            && matches!(name.as_str(), "vertex_indices" | "vertex_index");
                        let mut indices = is_face_indices.then(Vec::new);
                        for _ in 0..count {
                            let value = body.read_value(format, *item_type)?;
                            if let Some(indices) = &mut indices {
                                let index = nonnegative_integer(value, "PLY vertex index")?;
                                indices.push(i32::try_from(index).map_err(|_| {
                                    invalid_file("PLY vertex index exceeds supported range")
                                })?);
                            }
                        }
                        if let Some(indices) = indices {
                            face_indices = Some(indices);
                        }
                    }
                }
            }
            if element.name == "vertex" {
                let [Some(x), Some(y), Some(z)] = coordinates else {
                    return Err(invalid_file("PLY vertex is missing x, y, or z"));
                };
                let coordinates = [x, y, z];
                if coordinates.iter().any(|coordinate| !coordinate.is_finite()) {
                    return Err(invalid_file("PLY contains a non-finite vertex coordinate"));
                }
                vertices.push(coordinates);
            } else if element.name == "face" {
                if let Some(indices) = face_indices {
                    faces.push(indices);
                }
            }
        }
    }

    if vertices.is_empty() || faces.is_empty() {
        return Err(invalid_file("PLY must contain vertices and polygon faces"));
    }
    let mut shape = PolyhedronPrimitive::default();
    for vertex in &vertices {
        shape.add_vertex(*vertex);
    }
    for face in faces {
        if face
            .iter()
            .any(|index| *index < 0 || *index as usize >= vertices.len())
        {
            return Err(invalid_file("PLY face references an out-of-range vertex"));
        }
        shape.add_face(face);
    }
    Ok(shape)
}

fn parse_ply_header(
    data: &[u8],
) -> Result<(PlyFormat, Vec<PlyElement>, usize), PolyhedronReaderError> {
    let mut lines = data.split_inclusive(|byte| *byte == b'\n');
    let mut offset = 0;
    let first_line = lines
        .next()
        .ok_or_else(|| invalid_file("PLY header is empty"))?;
    offset += first_line.len();
    if trim_line(first_line) != "ply" {
        return Err(invalid_file("PLY file must begin with the ply signature"));
    }

    let mut format = None;
    let mut elements: Vec<PlyElement> = Vec::new();
    let mut ended = false;
    for line in lines {
        offset += line.len();
        let line = trim_line(line);
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.is_empty() || fields[0] == "comment" || fields[0] == "obj_info" {
            continue;
        }
        match fields.as_slice() {
            ["format", "ascii", "1.0"] => format = Some(PlyFormat::Ascii),
            ["format", "binary_little_endian", "1.0"] => {
                format = Some(PlyFormat::BinaryLittleEndian)
            }
            ["format", "binary_big_endian", "1.0"] => format = Some(PlyFormat::BinaryBigEndian),
            ["element", name, count] => {
                let count = count
                    .parse::<usize>()
                    .map_err(|_| invalid_file("PLY element count is invalid"))?;
                elements.push(PlyElement {
                    name: (*name).to_owned(),
                    count,
                    properties: Vec::new(),
                });
            }
            ["property", data_type, name] => {
                let data_type = PlyScalarType::parse(data_type)
                    .ok_or_else(|| invalid_file("PLY property uses an unsupported scalar type"))?;
                let element = elements
                    .last_mut()
                    .ok_or_else(|| invalid_file("PLY property appears before any element"))?;
                element.properties.push(PlyProperty::Scalar {
                    name: (*name).to_owned(),
                    data_type,
                });
            }
            ["property", "list", count_type, item_type, name] => {
                let count_type = PlyScalarType::parse(count_type)
                    .filter(|data_type| data_type.is_integer())
                    .ok_or_else(|| invalid_file("PLY list count type must be an integer"))?;
                let item_type = PlyScalarType::parse(item_type)
                    .ok_or_else(|| invalid_file("PLY list uses an unsupported scalar type"))?;
                let element = elements
                    .last_mut()
                    .ok_or_else(|| invalid_file("PLY property appears before any element"))?;
                element.properties.push(PlyProperty::List {
                    name: (*name).to_owned(),
                    count_type,
                    item_type,
                });
            }
            ["end_header"] => {
                ended = true;
                break;
            }
            _ => return Err(invalid_file("PLY header contains an unsupported directive")),
        }
    }
    if !ended {
        return Err(invalid_file("PLY header is missing end_header"));
    }
    let format = format.ok_or_else(|| invalid_file("PLY header is missing its format"))?;
    if !elements.iter().any(|element| element.name == "vertex")
        || !elements.iter().any(|element| element.name == "face")
    {
        return Err(invalid_file(
            "PLY header must declare vertex and face elements",
        ));
    }
    Ok((format, elements, offset))
}

fn trim_line(line: &[u8]) -> &str {
    std::str::from_utf8(line)
        .unwrap_or("")
        .trim_end_matches(['\r', '\n'])
        .trim()
}

struct PlyBody<'a> {
    data: &'a [u8],
    offset: usize,
    ascii_tokens: Option<Vec<&'a str>>,
}

impl PlyBody<'_> {
    fn read_value(
        &mut self,
        format: PlyFormat,
        data_type: PlyScalarType,
    ) -> Result<f64, PolyhedronReaderError> {
        if format == PlyFormat::Ascii {
            let tokens = self
                .ascii_tokens
                .as_mut()
                .expect("ASCII format has tokenized body");
            let token = tokens
                .get(self.offset)
                .ok_or_else(|| invalid_file("ASCII PLY data is truncated"))?;
            self.offset += 1;
            return token
                .parse::<f64>()
                .map_err(|_| invalid_file("ASCII PLY contains an invalid scalar"));
        }

        let width = data_type.byte_width();
        let end = self
            .offset
            .checked_add(width)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| invalid_file("binary PLY data is truncated"))?;
        let bytes = &self.data[self.offset..end];
        self.offset = end;
        let little_endian = format == PlyFormat::BinaryLittleEndian;
        Ok(match data_type {
            PlyScalarType::I8 => i8::from_ne_bytes([bytes[0]]) as f64,
            PlyScalarType::U8 => bytes[0] as f64,
            PlyScalarType::I16 => {
                let bytes = [bytes[0], bytes[1]];
                if little_endian {
                    i16::from_le_bytes(bytes) as f64
                } else {
                    i16::from_be_bytes(bytes) as f64
                }
            }
            PlyScalarType::U16 => {
                let bytes = [bytes[0], bytes[1]];
                if little_endian {
                    u16::from_le_bytes(bytes) as f64
                } else {
                    u16::from_be_bytes(bytes) as f64
                }
            }
            PlyScalarType::I32 => {
                let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
                if little_endian {
                    i32::from_le_bytes(bytes) as f64
                } else {
                    i32::from_be_bytes(bytes) as f64
                }
            }
            PlyScalarType::U32 => {
                let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
                if little_endian {
                    u32::from_le_bytes(bytes) as f64
                } else {
                    u32::from_be_bytes(bytes) as f64
                }
            }
            PlyScalarType::F32 => read_f32(bytes, little_endian) as f64,
            PlyScalarType::F64 => read_f64(bytes, little_endian),
        })
    }
}

fn read_f32(bytes: &[u8], little_endian: bool) -> f32 {
    let bytes: [u8; 4] = bytes.try_into().expect("4-byte scalar");
    if little_endian {
        f32::from_le_bytes(bytes)
    } else {
        f32::from_be_bytes(bytes)
    }
}

fn read_f32_le(bytes: &[u8]) -> f32 {
    f32::from_le_bytes(bytes.try_into().expect("4-byte scalar"))
}

fn read_f64(bytes: &[u8], little_endian: bool) -> f64 {
    let bytes: [u8; 8] = bytes.try_into().expect("8-byte scalar");
    if little_endian {
        f64::from_le_bytes(bytes)
    } else {
        f64::from_be_bytes(bytes)
    }
}

fn parse_finite_float(value: &str, field: &str) -> Result<f64, PolyhedronReaderError> {
    let value = value
        .parse::<f64>()
        .map_err(|_| invalid_file(format!("{field} is invalid")))?;
    if !value.is_finite() {
        return Err(invalid_file(format!("{field} is not finite")));
    }
    Ok(value)
}

fn nonnegative_integer(value: f64, field: &str) -> Result<usize, PolyhedronReaderError> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > usize::MAX as f64 {
        return Err(invalid_file(format!(
            "{field} is not a nonnegative integer"
        )));
    }
    Ok(value as usize)
}

fn invalid_file(message: impl Into<String>) -> PolyhedronReaderError {
    PolyhedronReaderError::InvalidFile(message.into())
}

#[cfg(test)]
mod tests {
    use super::{read_ply, read_stl};
    use crate::model::PolyhedronPrimitive;

    fn assert_single_triangle(shape: &PolyhedronPrimitive) {
        assert_eq!(shape.vertex_count(), 3);
        assert_eq!(shape.face_count(), 1);
        assert_eq!(shape.vertex(0), Some([0.0, 0.0, 0.0]));
        assert_eq!(shape.vertex(1), Some([1.0, 0.0, 0.0]));
        assert_eq!(shape.vertex(2), Some([0.0, 1.0, 0.0]));
        assert_eq!(shape.face(0), Some(&[0, 1, 2][..]));
    }

    #[test]
    fn reads_ascii_stl_triangles() {
        let data = b"solid sample\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid sample\n";
        let shape = read_stl(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_ascii_stl_with_uppercase_keywords() {
        let data = b"SOLID sample\nFACET NORMAL 0 0 1\nOUTER LOOP\nVERTEX 0 0 0\nVERTEX 1 0 0\nVERTEX 0 1 0\nENDLOOP\nENDFACET\nENDSOLID sample\n";
        let shape = read_stl(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn solid_name_does_not_create_a_vertex() {
        let data = b"solid vertex 9 9 9\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid vertex 9 9 9\n";
        let shape = read_stl(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn ascii_stl_matches_vtk_facet_and_vertex_token_tolerance() {
        let data = b"solid sample\nfacet ignored normal values\nouter loop\nvertex 0 0 0 extra\nvertex 1 0 0 extra\nvertex 0 1 0 extra\nendloop\nendfacet\nendsolid sample\n";
        let shape = read_stl(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn ascii_stl_matches_vtk_trailing_structural_token_tolerance() {
        let data = b"solid sample\nfacet normal 0 0 1\nouter loop extra\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop extra\nendfacet extra\nendsolid sample\n";
        let shape = read_stl(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn merges_vertices_across_ascii_stl_solids_like_vtk() {
        let data = b"solid first\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid first\nsolid second\nfacet normal 0 0 1\nouter loop\nvertex 0 0 1\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid second\n";
        let shape = read_stl(data).unwrap();
        assert_eq!(
            (0..shape.vertex_count())
                .map(|index| shape.vertex(index).unwrap())
                .collect::<Vec<_>>(),
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0]
            ]
        );
        assert_eq!(shape.face(0), Some(&[0, 1, 2][..]));
        assert_eq!(shape.face(1), Some(&[3, 1, 2][..]));
    }

    #[test]
    fn rejects_unrecognized_ascii_stl_lines() {
        let data = b"solid sample\n# vertex 9 9 9\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid sample\n";
        assert!(read_stl(data).is_err());

        let long_invalid_ascii = format!("solid sample\n# {}\n", "x".repeat(512));
        assert!(read_stl(long_invalid_ascii.as_bytes()).is_err());
    }

    #[test]
    fn reads_binary_stl_triangles() {
        let mut data = vec![0; 134];
        data[80..84].copy_from_slice(&1u32.to_le_bytes());
        for (index, coordinate) in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
            .into_iter()
            .enumerate()
        {
            let offset = 96 + index * 12;
            for (axis, value) in coordinate.into_iter().enumerate() {
                data[offset + axis * 4..offset + axis * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        let shape = read_stl(&data).unwrap();
        assert_single_triangle(&shape);

        data[..12].copy_from_slice(b"solid binary");
        let shape = read_stl(&data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn binary_stl_with_printable_payload_and_solid_header_matches_vtk_detection() {
        let mut data = b"solid printable binary STL".to_vec();
        data.resize(80, b' ');
        data.extend_from_slice(&1u32.to_le_bytes());

        let normal = f32::from_bits(0x3f20_2020).to_le_bytes();
        data.extend_from_slice(&normal);
        data.extend_from_slice(&normal);
        data.extend_from_slice(&normal);
        for coordinate in [
            [0x3f21_2020, 0x3f20_2120, 0x3f20_2220],
            [0x3f20_2221, 0x3f22_2020, 0x3f20_2220],
            [0x3f21_2321, 0x3f23_2020, 0x3f20_2320],
        ] {
            for value in coordinate {
                data.extend_from_slice(&f32::from_bits(value).to_le_bytes());
            }
        }
        data.extend_from_slice(b"XY");

        assert_eq!(data.len(), 134);
        assert!(data[84..].iter().all(|byte| (0x20..=0x7e).contains(byte)));
        assert!(read_stl(&data).is_err());
    }

    #[test]
    fn binary_stl_uses_available_facet_records_like_vtk() {
        let make_data = |declared_count: u32, actual_count: usize| {
            let mut data = vec![0; 84 + actual_count * 50 + 17];
            data[80..84].copy_from_slice(&declared_count.to_le_bytes());
            for triangle in 0..actual_count {
                let facet_start = 84 + triangle * 50;
                for (index, coordinate) in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
                    .into_iter()
                    .enumerate()
                {
                    let offset = facet_start + 12 + index * 12;
                    for (axis, value) in coordinate.into_iter().enumerate() {
                        data[offset + axis * 4..offset + axis * 4 + 4]
                            .copy_from_slice(&value.to_le_bytes());
                    }
                }
            }
            data
        };

        for (declared_count, actual_count) in [(1, 2), (2, 1), (0, 1)] {
            let shape = read_stl(&make_data(declared_count, actual_count)).unwrap();
            assert_eq!(shape.face_count(), actual_count);
            assert_eq!(shape.vertex_count(), 3);
        }
    }

    #[test]
    fn merges_shared_vertices_across_binary_stl_facets_like_vtk() {
        let triangles = [
            [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [[0.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        ];
        let mut data = vec![0; 84 + triangles.len() * 50];
        data[80..84].copy_from_slice(&(triangles.len() as u32).to_le_bytes());
        for (triangle_index, triangle) in triangles.iter().enumerate() {
            let facet_start = 84 + triangle_index * 50 + 12;
            for (vertex_index, vertex) in triangle.iter().enumerate() {
                let vertex_start = facet_start + vertex_index * 12;
                for (axis, coordinate) in vertex.iter().enumerate() {
                    let start = vertex_start + axis * 4;
                    data[start..start + 4].copy_from_slice(&coordinate.to_le_bytes());
                }
            }
        }

        let shape = read_stl(&data).unwrap();
        assert_eq!(
            (0..shape.vertex_count())
                .map(|index| shape.vertex(index).unwrap())
                .collect::<Vec<_>>(),
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0]
            ]
        );
        assert_eq!(shape.face(0), Some(&[0, 1, 2][..]));
        assert_eq!(shape.face(1), Some(&[0, 2, 3][..]));
    }

    #[test]
    fn reads_ascii_ply_vertex_index_alias_with_extra_vertex_fields() {
        let data = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nelement face 1\nproperty list uchar int vertex_index\nend_header\n0 0 0 255\n1 0 0 255\n0 1 0 255\n3 0 1 2\n";
        let shape = read_ply(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_ascii_ply_comment_and_obj_info_directives_like_vtk() {
        let data = b"ply\nformat ascii 1.0\ncomment differential fixture\nobj_info vendor=probe\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n";
        let shape = read_ply(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_ascii_ply_with_ignored_elements_like_vtk() {
        let data = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement edge 1\nproperty int vertex1\nproperty int vertex2\nproperty uchar flags\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n0 1 9\n3 0 1 2\n";
        let shape = read_ply(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_ascii_ply_with_ignored_list_elements_like_vtk() {
        let data = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement edge 1\nproperty list uchar int vertex_indices\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n2 0 1\n3 0 1 2\n";
        let shape = read_ply(data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_binary_ply_with_ignored_list_elements_like_vtk() {
        let header = b"ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement edge 1\nproperty list uchar int vertex_indices\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n";
        let mut data = header.to_vec();
        for coordinates in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for coordinate in coordinates {
                data.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        data.push(2);
        for index in [0i32, 1] {
            data.extend_from_slice(&index.to_le_bytes());
        }
        data.push(3);
        for index in [0i32, 1, 2] {
            data.extend_from_slice(&index.to_le_bytes());
        }

        let shape = read_ply(&data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_two_vertex_ascii_ply_faces_like_vtk() {
        let data = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n2 0 1\n";
        let shape = read_ply(data).unwrap();

        assert_eq!(shape.vertex_count(), 3);
        assert_eq!(shape.face_count(), 1);
        assert_eq!(shape.face(0), Some(&[0, 1][..]));
    }

    #[test]
    fn reads_ascii_ply_faces_with_zero_to_three_indices_like_vtk() {
        let data = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 4\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n0\n1 0\n2 0 1\n3 0 1 2\n";
        let shape = read_ply(data).unwrap();

        assert_eq!(shape.face_count(), 4);
        assert_eq!(shape.face(0), Some(&[][..]));
        assert_eq!(shape.face(1), Some(&[0][..]));
        assert_eq!(shape.face(2), Some(&[0, 1][..]));
        assert_eq!(shape.face(3), Some(&[0, 1, 2][..]));
    }

    #[test]
    fn reads_binary_ply_faces_with_zero_to_three_indices_like_vtk() {
        let header = b"ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 4\nproperty list uchar int vertex_indices\nend_header\n";
        let mut data = header.to_vec();
        for coordinates in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for coordinate in coordinates {
                data.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        for indices in [vec![], vec![0i32], vec![0, 1], vec![0, 1, 2]] {
            data.push(indices.len() as u8);
            for index in indices {
                data.extend_from_slice(&index.to_le_bytes());
            }
        }
        let shape = read_ply(&data).unwrap();

        assert_eq!(shape.face_count(), 4);
        assert_eq!(shape.face(0), Some(&[][..]));
        assert_eq!(shape.face(1), Some(&[0][..]));
        assert_eq!(shape.face(2), Some(&[0, 1][..]));
        assert_eq!(shape.face(3), Some(&[0, 1, 2][..]));
    }

    #[test]
    fn reads_binary_little_endian_ply_vertex_index_alias() {
        let header = b"ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_index\nend_header\n";
        let mut data = header.to_vec();
        for coordinates in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for coordinate in coordinates {
                data.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        data.push(3);
        for index in [0i32, 1, 2] {
            data.extend_from_slice(&index.to_le_bytes());
        }
        let shape = read_ply(&data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_binary_big_endian_ply_vertex_index_alias() {
        let header = b"ply\nformat binary_big_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_index\nend_header\n";
        let mut data = header.to_vec();
        for coordinates in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for coordinate in coordinates {
                data.extend_from_slice(&coordinate.to_be_bytes());
            }
        }
        data.push(3);
        for index in [0i32, 1, 2] {
            data.extend_from_slice(&index.to_be_bytes());
        }
        let shape = read_ply(&data).unwrap();
        assert_single_triangle(&shape);
    }

    #[test]
    fn reads_binary_ply_mixed_scalar_widths_and_property_order_like_vtk() {
        let header = b"ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty uchar red\nproperty float64 z\nproperty float64 x\nproperty float64 y\nelement face 1\nproperty float confidence\nproperty list uchar int16 vertex_indices\nend_header\n";
        let mut data = header.to_vec();
        for (x, y, z) in [(0.0f64, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)] {
            data.push(255);
            for coordinate in [z, x, y] {
                data.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.push(3);
        for index in [0i16, 1, 2] {
            data.extend_from_slice(&index.to_le_bytes());
        }

        let shape = read_ply(&data).unwrap();
        assert_single_triangle(&shape);
        assert_eq!(
            (0..shape.vertex_count())
                .map(|index| shape.vertex(index).unwrap())
                .collect::<Vec<_>>(),
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        );
    }

    #[test]
    fn merges_shared_ascii_stl_vertices() {
        let data = b"solid sample\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 0 1 0\nvertex 0 0 1\nendloop\nendfacet\nendsolid sample\n";
        let shape = read_stl(data).unwrap();
        assert_eq!(shape.vertex_count(), 4);
        assert_eq!(shape.face_count(), 2);
        assert_eq!(
            (0..shape.vertex_count())
                .map(|index| shape.vertex(index).unwrap())
                .collect::<Vec<_>>(),
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0]
            ]
        );
        assert_eq!(shape.face(0), Some(&[0, 1, 2][..]));
        assert_eq!(shape.face(1), Some(&[0, 2, 3][..]));
    }

    #[test]
    fn rejects_invalid_stl_and_ply_data() {
        assert!(read_stl(b"solid empty\nendsolid empty\n").is_err());
        assert!(read_ply(b"ply\nformat ascii 1.0\nend_header\n").is_err());
    }

    #[test]
    fn rejects_empty_and_vertex_only_ply_files_like_native_reader() {
        let empty = b"ply\nformat ascii 1.0\nelement vertex 0\nproperty float x\nproperty float y\nproperty float z\nelement face 0\nproperty list uchar int vertex_indices\nend_header\n";
        let vertices_only = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 0\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n";

        assert!(read_ply(empty).is_err());
        assert!(read_ply(vertices_only).is_err());
    }

    #[test]
    fn rejects_ascii_stl_vertices_outside_a_complete_facet() {
        let vertices_without_facet =
            b"solid sample\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendsolid sample\n";
        let missing_outer_loop = b"solid sample\nfacet normal 0 0 1\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid sample\n";
        let missing_endloop = b"solid sample\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendfacet\nendsolid sample\n";
        let missing_endsolid = b"solid sample\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\n";
        assert!(read_stl(vertices_without_facet).is_err());
        assert!(read_stl(missing_outer_loop).is_err());
        assert!(read_stl(missing_endloop).is_err());
        assert!(read_stl(missing_endsolid).is_err());
    }
}
