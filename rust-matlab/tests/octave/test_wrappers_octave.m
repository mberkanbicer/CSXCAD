function test_wrappers_octave()
% TEST_WRAPPERS_OCTAVE  RM-032/RM-033: exercise the native CSXCAD M-file API in Octave.
%
% The Rust MATLAB delivery keeps the native M-file API unchanged, so the
% contracts exercised here are the native ones. Every check states the native
% source it pins; where the native behaviour is a documented defect (the Octave
% XML writer's format-string handling) it is pinned as a defect instead of being
% silently "fixed" by the migration.
%
% The full model written at the end is re-read by
% `tests/octave/check_written_xml.py` through the Rust CSXCAD Python package when
% `RM032_XML_OUT` is set, which proves the M-file output is still consumable by
% the Rust core.
%
% Run: octave --no-gui --quiet --eval "test_wrappers_octave" (see run_octave_tests.sh)

  csxcad = fullfile(fileparts(mfilename('fullpath')), '..', '..', '..', 'matlab');
  addpath(csxcad);

  check_init_and_grid();
  check_properties_and_primitives();
  check_probe_and_dump();
  check_excitation();
  check_mesh_helpers();
  check_utilities();
  check_xml_writer();
  check_exporters();
  check_error_identifiers();
  check_rust_round_trip();

  printf('RM-032/RM-033 Octave M-file API: all wrapper checks passed\n');
end

% ---------------------------------------------------------------------------
function check_init_and_grid()
  % InitCSX.m: Properties = [], ATTRIBUTE.CoordSystem = 0, name/value pairs
  csx = InitCSX();
  assert_true(isempty(csx.Properties), 'InitCSX must start with no properties');
  assert_equal(csx.ATTRIBUTE.CoordSystem, 0, 'InitCSX default coordinate system');

  csx = InitCSX('CoordSystem', 1);
  assert_equal(csx.ATTRIBUTE.CoordSystem, 1, 'InitCSX coordinate system argument');

  % InitCSX indexes varargin{n+1} without checking the pair count: an odd
  % argument count fails through the normal missing-element error (audit note).
  assert_throws(@() InitCSX('CoordSystem'), 'InitCSX odd argument count must fail');

  % DefineRectGrid.m: DeltaUnit, CoordSystem copy, lines, x/r and y/a selection
  mesh = struct('x', 0:1:10, 'y', 0:2:8, 'z', [0 1 2 5]);
  csx = DefineRectGrid(InitCSX(), 1e-3, mesh);
  assert_equal(csx.RectilinearGrid.ATTRIBUTE.DeltaUnit, 1e-3, 'DefineRectGrid unit');
  assert_equal(csx.RectilinearGrid.ATTRIBUTE.CoordSystem, 0, 'grid coordinate system');
  assert_equal(csx.RectilinearGrid.XLines, 0:1:10, 'x lines');
  assert_equal(csx.RectilinearGrid.YLines, 0:2:8, 'y lines');
  assert_equal(csx.RectilinearGrid.ZLines, [0 1 2 5], 'z lines');

  cyl = DefineRectGrid(InitCSX('CoordSystem', 1), 1, ...
                       struct('r', [0 1 2], 'a', [0 pi/2 pi], 'z', [0 1]));
  assert_equal(cyl.RectilinearGrid.XLines, [0 1 2], 'cylindrical r lines');
  assert_equal(cyl.RectilinearGrid.YLines, [0 pi/2 pi], 'cylindrical a lines');

  % missing x/r raises the explicit message
  assert_throws(@() DefineRectGrid(InitCSX(), 1, struct('y', 0:1, 'z', 0:1)), ...
                'DefineRectGrid without x/r must fail');
end

function check_properties_and_primitives()
  csx = DefineRectGrid(InitCSX(), 1, struct('x', 0:10, 'y', 0:10, 'z', 0:5));

  % AddMaterial + SetMaterialProperty/SetMaterialWeight (SetPropertyArgs).
  % The M-file API stores the *XML* attribute names verbatim, so `Epsilon`
  % (not the Python API's `epsilon`) is what the native reader interprets:
  % CSPropMaterial::ReadFromXML reads elem.Attribute("Epsilon")
  % (src/CSPropMaterial.cpp:298-305) and TiXmlElement::Attribute is an exact,
  % case-sensitive comparison (src/ParameterObjects.cpp:65).
  csx = AddMaterial(csx, 'substrate');
  csx = SetMaterialProperty(csx, 'substrate', 'Epsilon', 2.5, 'Kappa', 0.1);
  assert_equal(csx.Properties.Material{1}.ATTRIBUTE.Name, 'substrate', 'material name');
  assert_equal(csx.Properties.Material{1}.Property.ATTRIBUTE.Epsilon, 2.5, 'material epsilon');
  assert_equal(csx.Properties.Material{1}.Property.ATTRIBUTE.Kappa, 0.1, 'material kappa');
  % the key is stored verbatim, so a lowercase spelling survives in the struct
  % but is ignored by the (case-sensitive) reader
  csx = SetMaterialProperty(csx, 'substrate', 'epsilon', 9.0);
  assert_equal(csx.Properties.Material{1}.Property.ATTRIBUTE.epsilon, 9.0, ...
               'M-file API stores attribute keys verbatim');
  csx = SetMaterialWeight(csx, 'substrate', 'Epsilon', 'x*y');
  assert_true(strcmp(csx.Properties.Material{1}.Weight.ATTRIBUTE.Epsilon, 'x*y'), ...
              'material weight expression');

  % AddMetal + AddBox: priority, endpoints, transformation nesting
  csx = AddMetal(csx, 'metal');
  csx = AddBox(csx, 'metal', 10, [0 0 0], [1 2 3], ...
               'Transform', {'Translate', '0,0,1'});
  box = csx.Properties.Metal{1}.Primitives.Box{1};
  assert_equal(box.ATTRIBUTE.Priority, 10, 'box priority');
  assert_equal(box.P1.ATTRIBUTE.X, 0, 'box P1 x');
  assert_equal(box.P1.ATTRIBUTE.Y, 0, 'box P1 y');
  assert_equal(box.P1.ATTRIBUTE.Z, 0, 'box P1 z');
  assert_equal(box.P2.ATTRIBUTE.X, 1, 'box P2 x');
  assert_equal(box.P2.ATTRIBUTE.Y, 2, 'box P2 y');
  assert_equal(box.P2.ATTRIBUTE.Z, 3, 'box P2 z');
  assert_true(strcmp(box.Transformation.Translate.ATTRIBUTE.Argument, '0,0,1'), ...
              'AddPrimitiveArgs transform nesting');

  % AddConductingSheet: zero thickness/conductivity falls back to a metal,
  % negative values error (AddConductingSheet.m:27-37)
  csx = AddConductingSheet(csx, 'sheet', 56e6, 35e-6);
  assert_true(isfield(csx.Properties, 'ConductingSheet'), 'conducting sheet property');
  assert_equal(csx.Properties.ConductingSheet{1}.ATTRIBUTE.Conductivity, 56e6, ...
               'conducting sheet conductivity');
  assert_equal(csx.Properties.ConductingSheet{1}.ATTRIBUTE.Thickness, 35e-6, ...
               'conducting sheet thickness');
  before = numel(csx.Properties.Metal);
  csx = AddConductingSheet(csx, 'sheet_zero', 0, 0);
  assert_equal(numel(csx.Properties.Metal), before + 1, ...
               'zero conducting sheet must fall back to AddMetal');

  % AddLumpedElement: direction through DirChar2Int, options as attributes
  csx = AddLumpedElement(csx, 'le', 'x', 'R', 50, 'C', 1e-12);
  assert_equal(csx.Properties.LumpedElement{1}.ATTRIBUTE.Direction, 0, 'LE direction');
  assert_equal(csx.Properties.LumpedElement{1}.ATTRIBUTE.R, 50, 'LE resistance');
  assert_equal(csx.Properties.LumpedElement{1}.ATTRIBUTE.C, 1e-12, 'LE capacitance');

  % primitive constructors -------------------------------------------------
  csx = AddCylinder(csx, 'metal', 5, [0 0 0], [0 0 1], 0.5);
  cyl = csx.Properties.Metal{1}.Primitives.Cylinder{1};
  assert_equal(cyl.ATTRIBUTE.Radius, 0.5, 'cylinder radius');
  assert_equal(cyl.P2.ATTRIBUTE.Z, 1, 'cylinder stop');

  csx = AddCylindricalShell(csx, 'metal', 5, [0 0 0], [0 0 1], 1, 0.1);
  assert_equal(csx.Properties.Metal{1}.Primitives.CylindricalShell{1}.ATTRIBUTE.ShellWidth, ...
               0.1, 'cylindrical shell width');

  csx = AddSphere(csx, 'metal', 5, [1 2 3], 0.75);
  assert_equal(csx.Properties.Metal{1}.Primitives.Sphere{1}.ATTRIBUTE.Radius, 0.75, ...
               'sphere radius');
  assert_equal(csx.Properties.Metal{1}.Primitives.Sphere{1}.Center.ATTRIBUTE.Y, 2, ...
               'sphere center');

  csx = AddSphericalShell(csx, 'metal', 5, [0 0 0], 1, 0.2);
  assert_equal(csx.Properties.Metal{1}.Primitives.SphericalShell{1}.ATTRIBUTE.ShellWidth, ...
               0.2, 'spherical shell width');

  csx = AddPoint(csx, 'metal', 3, [1 1 1]);
  assert_equal(csx.Properties.Metal{1}.Primitives.Point{1}.ATTRIBUTE.Z, 1, 'point z');

  csx = AddPolygon(csx, 'metal', 5, 'z', 0, [0 1 1 0; 0 0 1 1]);
  poly = csx.Properties.Metal{1}.Primitives.Polygon{1};
  assert_equal(poly.ATTRIBUTE.NormDir, 2, 'polygon normal direction');
  assert_equal(numel(poly.Vertex), 4, 'polygon vertex count');

  csx = AddLinPoly(csx, 'metal', 5, 'z', 0, [0 1 1 0; 0 0 1 1], 2);
  assert_equal(csx.Properties.Metal{1}.Primitives.LinPoly{1}.ATTRIBUTE.Length, 2, ...
               'linpoly length');

  csx = AddRotPoly(csx, 'metal', 5, 'z', [0 1; 0 1], 'x', [0 pi]);
  rotpoly = csx.Properties.Metal{1}.Primitives.RotPoly{1};
  assert_equal(rotpoly.ATTRIBUTE.RotAxisDir, 0, 'rotpoly axis');
  assert_equal(numel(rotpoly.Vertex), 2, 'rotpoly vertex count');
  assert_equal(rotpoly.Angles.ATTRIBUTE.Stop, pi, 'rotpoly stop angle');

  csx = AddCurve(csx, 'metal', 5, [0 1 2; 0 1 2; 0 0 0]);
  assert_equal(numel(csx.Properties.Metal{1}.Primitives.Curve{1}.Vertex), 3, ...
               'curve vertex count');

  csx = AddWire(csx, 'metal', 5, [0 1; 0 1; 0 1], 0.1);
  wire = csx.Properties.Metal{1}.Primitives.Wire{1};
  assert_equal(wire.ATTRIBUTE.WireRadius, 0.1, 'wire radius');
  assert_equal(numel(wire.Vertex), 2, 'wire vertex count');

  csx = AddPolyhedron(csx, 'metal', 5, ...
                      {[0 0 0], [1 0 0], [1 1 0], [0 1 0]}, ...
                      {[1 2 3], [1 3 4]});
  ph = csx.Properties.Metal{1}.Primitives.Polyhedron{1};
  assert_equal(numel(ph.Vertex), 4, 'polyhedron vertex count');
  assert_equal(numel(ph.Face), 2, 'polyhedron face count');

  stl = fullfile(fileparts(mfilename('fullpath')), '..', '..', '..', 'python', 'tests', 'sphere.stl');
  csx = ImportSTL(csx, 'metal', 5, stl);
  reader = csx.Properties.Metal{1}.Primitives.PolyhedronReader{1};
  assert_true(strcmp(reader.ATTRIBUTE.FileType, 'STL'), 'ImportSTL file type');
  assert_true(strcmp(reader.ATTRIBUTE.FileName, stl), 'ImportSTL filename');
end

function check_probe_and_dump()
  csx = InitCSX();
  csx = AddProbe(csx, 'ut', 0, 'Frequency', 1e9, 'Weight', '1');
  probe = csx.Properties.ProbeBox{1};
  assert_equal(probe.ATTRIBUTE.Type, 0, 'probe type');
  assert_equal(probe.FD_Samples, 1e9, 'probe frequency');
  assert_true(strcmp(probe.ATTRIBUTE.Weight, '1'), 'probe weight');
  assert_equal(numel(fieldnames(probe)), 2, 'probe has ATTRIBUTE and FD_Samples');

  % unknown probe option: the native warning formats the *value* (audit note),
  % so the numeric 42 is converted to char(42) == '*'
  lastwarn('');
  csx = AddProbe(csx, 'ut2', 0, 'NotAnOption', 42);
  [message, identifier] = lastwarn();
  assert_true(strcmp(identifier, 'CSXCAD:AddProbe'), 'probe unknown option identifier');
  assert_true(~isempty(strfind(message, '"*"')), ...
              'native AddProbe warning reports char(value), not the key');

  % AddDump.m: DumpMode defaults to node interpolation, Frequency -> FD_Samples
  csx = AddDump(csx, 'Et', 'DumpType', 10, 'Frequency', [1e9 2e9], 'FileType', 1);
  dump = csx.Properties.DumpBox{1};
  assert_equal(dump.ATTRIBUTE.DumpMode, 1, 'dump default mode');
  assert_equal(dump.ATTRIBUTE.DumpType, 10, 'dump type');
  assert_equal(dump.ATTRIBUTE.FileType, 1, 'dump file type');
  assert_equal(dump.FD_Samples, [1e9 2e9], 'dump frequencies');
end

function check_excitation()
  csx = InitCSX();
  csx = AddExcitation(csx, 'exc', 0, [0 0 1]);
  assert_equal(csx.Properties.Excitation{1}.ATTRIBUTE.Type, 0, 'excitation type');
  assert_equal(csx.Properties.Excitation{1}.ATTRIBUTE.Excite, [0 0 1], 'excitation vector');

  % AddPlaneWaveExcite wraps AddExcitation with type 10 (AddPlaneWaveExcite.m)
  csx = AddPlaneWaveExcite(csx, 'pw', [0 1 0], [0 0 1]);
  pw = csx.Properties.Excitation{2};
  assert_equal(pw.ATTRIBUTE.Type, 10, 'plane wave type');
  assert_equal(pw.ATTRIBUTE.Excite, [0 0 1], 'plane wave polarisation');
  assert_equal(pw.ATTRIBUTE.PropDir, [0 1 0], 'plane wave propagation direction');
  assert_equal(pw.ATTRIBUTE.Frequency, 0, 'plane wave default frequency');

  % SetExcitationWeight writes the Weight attributes of the named excitation
  csx = SetExcitationWeight(csx, 'exc', {1, 1, 1});
  assert_equal(csx.Properties.Excitation{1}.Weight.ATTRIBUTE.X, 1, 'excitation weight x');
end

function check_mesh_helpers()
  % SmoothMeshLines.m: sorted, unique, endpoints preserved, max spacing held
  lines = SmoothMeshLines([0 1], 0.3);
  assert_true(all(diff(lines) > 0), 'smoothed lines must be strictly increasing');
  assert_equal(lines(1), 0, 'smoothed first line');
  assert_equal(lines(end), 1, 'smoothed last line');
  assert_true(max(diff(lines)) <= 0.3 + 1e-12, 'smoothed max spacing');

  % fixed lines survive smoothing
  fixed = SmoothMeshLines([0 0.123 1], 0.5);
  assert_true(any(abs(fixed - 0.123) < 1e-12), 'fixed line must be preserved');

  % AnalyseMesh.m returns the documented fields
  stats = AnalyseMesh(fixed);
  for name = {'numLines', 'max_res', 'min_res', 'homogeneous', 'symmetric', 'max_ratio'}
    assert_true(isfield(stats, name{1}), ['AnalyseMesh field ' name{1}]);
  end
  assert_equal(stats.numLines, numel(fixed), 'AnalyseMesh line count');

  % CheckMesh.m: the quiet flag also suppresses the returned errors (audit note)
  [count, positions, types] = CheckMesh([0 2], 0, 1, 1.3, 0);
  assert_equal(count, 1, 'CheckMesh error count (reporting on)');
  assert_equal(positions, 1, 'CheckMesh error position');
  assert_equal(types, 1, 'CheckMesh error type (max resolution)');
  [count, positions, types] = CheckMesh([0 2], 0, 1, 1.3, 1);
  assert_equal(count, 0, 'CheckMesh quiet suppresses the returned errors');
  assert_true(isempty(positions) && isempty(types), 'CheckMesh quiet positions/types');

  % SmoothMesh.m smooths every axis of a mesh struct
  mesh = SmoothMesh(struct('x', [0 10], 'y', [0 10], 'z', [0 5]), 1, 1.5);
  assert_true(max(diff(mesh.x)) <= 1 + 1e-12, 'SmoothMesh x spacing');
  assert_true(max(diff(mesh.y)) <= 1 + 1e-12, 'SmoothMesh y spacing');
  assert_true(max(diff(mesh.z)) <= 1 + 1e-12, 'SmoothMesh z spacing');
end

function check_utilities()
  assert_equal(DirChar2Int('x'), 0, 'DirChar2Int x');
  assert_equal(DirChar2Int('r'), 0, 'DirChar2Int r');
  assert_equal(DirChar2Int('a'), 1, 'DirChar2Int a');
  assert_equal(DirChar2Int('z'), 2, 'DirChar2Int z');
  assert_equal(DirChar2Int(2), 2, 'DirChar2Int numeric');
  assert_throws(@() DirChar2Int('q'), 'DirChar2Int invalid direction');

  assert_true(isOctave(), 'isOctave must be true in Octave');

  % searchBinary concatenates each search-path entry with the binary name
  % directly (searchBinary.m:33-38), so the entry has to end with a separator;
  % it only checks exist(...,'file'), the executable bit is not consulted.
  dir = tempname();
  mkdir(dir);
  stub = fullfile(dir, 'csxcad-probe-binary');
  fid = fopen(stub, 'w');
  fprintf(fid, '#!/bin/sh\nexit 0\n');
  fclose(fid);
  assert_true(strcmp(searchBinary('csxcad-probe-binary', {[dir '/']}, false), stub), ...
              'searchBinary must find a binary in the supplied path');
  assert_true(isempty(searchBinary('csxcad-probe-binary', {dir}, false)), ...
              'native searchBinary concatenates without a separator');
  assert_true(isempty(searchBinary('csxcad-no-such-binary', {[dir '/']}, false)), ...
              'searchBinary must return empty for a missing binary');
  assert_throws(@() searchBinary('csxcad-no-such-binary', {[dir '/']}, true), ...
                'searchBinary must throw when err_fail is set');
  delete(stub);
  rmdir(dir);
end

function check_xml_writer()
  csx = DefineRectGrid(InitCSX(), 1e-3, struct('x', 0:2, 'y', 0:2, 'z', 0:1));
  csx = AddMaterial(csx, 'substrate');
  csx = SetMaterialProperty(csx, 'substrate', 'Epsilon', 2.5);
  csx = AddMetal(csx, 'metal');
  csx = AddBox(csx, 'metal', 10, [0 0 0], [1 1 1]);
  csx = AddExcitation(csx, 'exc', 0, [0 0 1]);

  file = [tempname() '.xml'];
  WriteCSX(file, csx);
  assert_true(isfile(file), 'WriteCSX must create the file');
  text = fileread(file);

  assert_true(~isempty(strfind(text, '<ContinuousStructure')), 'XML root element');
  assert_true(~isempty(strfind(text, 'CoordSystem="0"')), 'XML coordinate system');
  assert_true(~isempty(strfind(text, '<RectilinearGrid')), 'XML grid element');
  assert_true(~isempty(strfind(text, 'DeltaUnit="0.001"')), 'XML delta unit');
  assert_true(~isempty(strfind(text, 'Name="substrate"')), 'XML property name');
  assert_true(~isempty(strfind(text, 'Epsilon="2.5"')), 'XML material value');
  assert_true(~isempty(strfind(text, 'Priority="10"')), 'XML primitive priority');
  assert_true(~isempty(strfind(text, '<Excitation')), 'XML excitation element');
  delete(file);

  % Native Octave writer defect, deliberately not changed by the migration:
  % struct_2_xml.m passes the assembled document as the fprintf *format*, so a
  % literal '%' in the content aborts the write (audit: "percent characters in
  % serialized content are a source-only formatting risk").
  percent = [tempname() '.xml'];
  assert_throws(@() WriteCSX(percent, InitCSX('Comment', '100%')), ...
                'native Octave writer treats percent content as a format string');
  if isfile(percent)
    delete(percent);
  end

  % The Octave writer does not escape XML text/attributes (unlike the MATLAB DOM
  % writer), so a raw ampersand survives; the XML reader therefore has to treat
  % such documents as the native ones.
  ampersand = [tempname() '.xml'];
  WriteCSX(ampersand, InitCSX('Comment', 'A&B'));
  text = fileread(ampersand);
  assert_true(~isempty(strfind(text, 'A&B')), 'Octave writer keeps raw ampersands');
  delete(ampersand);
end

function check_exporters()
  csx = DefineRectGrid(InitCSX(), 1e-3, struct('x', 0:10, 'y', 0:10, 'z', 0:5));
  csx = AddMetal(csx, 'metal');
  csx = AddBox(csx, 'metal', 1, [0 0 0], [5 5 0]);

  gerber = [tempname() '.gbr'];
  export_gerber(csx, gerber);
  assert_true(isfile(gerber), 'export_gerber must create a file');
  text = fileread(gerber);
  assert_true(~isempty(strfind(text, 'G04')) || ~isempty(strfind(text, 'X')), ...
              'export_gerber content');
  delete(gerber);

  pov = [tempname() '.pov'];
  export_povray(csx, pov);
  assert_true(isfile(pov), 'export_povray must create a file');
  text = fileread(pov);
  assert_true(~isempty(strfind(text, 'box')), 'export_povray must emit a box');
  delete(pov);
end

function check_error_identifiers()
  % Private helpers, exercised through the public API (AddBox -> Add2Property ->
  % GetPropertyType -> GetPropertyPosition).
  csx = InitCSX();
  assert_error_identifier(@() AddBox(csx, 'missing', 1, [0 0 0], [1 1 1]), ...
                          'CSXCAD:Add2Property');
  assert_error_identifier(@() AddBox(struct('ATTRIBUTE', struct()), 'x', 1, [0 0 0], [1 1 1]), ...
                          'CSXCAD:GetPropertyPosition');
  assert_error_identifier(@() AddConductingSheet(csx, 'cs', -1, 1e-6), ...
                          'CSXCAD:AddConductingSheet');
  assert_error_identifier(@() DirChar2Int('q'), 'CSXCAD:DirChar2Int');
  % duplicate property names are rejected by private/AddProperty.m:13
  csx = AddMaterial(csx, 'dup');
  assert_error_identifier(@() AddMaterial(csx, 'dup'), 'CSXCAD:AddProperty');
  % unknown property in SetMaterialProperty: GetPropertyType finds no type, so
  % private/GetPropertyPosition.m:28 reports the empty type
  assert_error_identifier(@() SetMaterialProperty(csx, 'nope', 'epsilon', 1), ...
                          'CSXCAD:GetPropertyPosition');

  % Message-only native errors (single-argument error(), no identifier):
  % AddProbe.m:51, private/AddProperty.m:26, AddDump.m:97, GetPropertyType.m:11.
  assert_error_message(@() AddProbe(csx, 7, 0), 'CSXCAD::AddProbe');
  assert_error_message(@() AddMaterial(csx, 'badkey', 1, 2), 'CSXCAD::AddProperty');
  assert_error_message(@() AddDump(csx, 'dump', 1, 2), 'CSXCAD::AddDump');
  assert_error_message(@() AddPropAttribute(csx, 7, 'a', 1), 'CSXCAD::GetPropertyType');
  % An unknown property name leaves GetPropertyType's type empty and
  % private/AddPropAttribute.m indexes CSX.Properties.('') before reaching its
  % own error branch (audit: "does not explicitly check for an empty type").
  assert_throws(@() AddPropAttribute(csx, 'nope', 'a', 1), ...
                'AddPropAttribute with an unknown property must fail');
  % the success path of the private property lookup helpers
  csx = AddPropAttribute(csx, 'dup', 'Note', 'hello');
  assert_true(strcmp(csx.Properties.Material{1}.Attributes.ATTRIBUTE.Note, 'hello'), ...
              'AddPropAttribute must store the attribute on the named property');
  % CreateDiscMaterial rejects Octave before touching HDF5 (CreateDiscMaterial.m:36)
  assert_error_message(@() CreateDiscMaterial([tempname() '.h5'], [], struct(), struct()), ...
                       'does not support Octave');
  assert_error_identifier(@() searchBinary('csxcad-no-such-binary', {}, true), ...
                          'CSXCAD:binary_location');
end

function check_rust_round_trip()
  out = getenv('RM032_XML_OUT');
  if isempty(out)
    return
  end
  csx = DefineRectGrid(InitCSX(), 1e-3, struct('x', 0:1:10, 'y', 0:1:10, 'z', 0:1:5));
  csx = AddMaterial(csx, 'substrate');
  csx = SetMaterialProperty(csx, 'substrate', 'Epsilon', 2.5, 'Kappa', 0.1);
  csx = AddMetal(csx, 'metal');
  csx = AddBox(csx, 'metal', 10, [0 0 0], [3 4 5]);
  csx = AddExcitation(csx, 'exc', 0, [0 0 1]);
  csx = AddProbe(csx, 'ut', 0, 'Frequency', 1e9);
  WriteCSX(out, csx);
  assert_true(isfile(out), 'round-trip XML was not written');
end

% ---------------------------------------------------------------------------
function assert_true(condition, message)
  if ~condition
    error('RM032:assert', '%s', message);
  end
end

function assert_equal(actual, expected, message)
  if ~isequal(actual, expected)
    error('RM032:assert', '%s: expected %s, got %s', message, ...
          mat2str(expected), mat2str(actual));
  end
end

function assert_throws(fn, message)
  try
    fn();
  catch
    return
  end
  error('RM032:assert', '%s (no error raised)', message);
end

function assert_error_identifier(fn, expected)
  try
    fn();
  catch err
    if ~strcmp(err.identifier, expected)
      error('RM032:assert', 'expected error identifier %s, got %s (%s)', ...
            expected, err.identifier, err.message);
    end
    return
  end
  error('RM032:assert', 'expected error identifier %s, no error raised', expected);
end

function assert_error_message(fn, expected)
  try
    fn();
  catch err
    if isempty(strfind([err.identifier ' ' err.message], expected))
      error('RM032:assert', 'expected error text %s, got %s (%s)', ...
            expected, err.identifier, err.message);
    end
    return
  end
  error('RM032:assert', 'expected error text %s, no error raised', expected);
end
