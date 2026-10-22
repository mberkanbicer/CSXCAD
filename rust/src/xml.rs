use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct XmlNode {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Self>,
    pub text: String,
}

impl XmlNode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            children: Vec::new(),
            text: String::new(),
        }
    }

    pub fn set_attribute(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some((_, current)) = self
            .attributes
            .iter_mut()
            .find(|(current, _)| current == &name)
        {
            *current = value;
        } else {
            self.attributes.push((name, value));
        }
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value.as_str()))
    }

    pub fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|child| child.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Self> {
        self.children.iter().filter(move |child| child.name == name)
    }

    pub fn to_xml(&self, output: &mut String) {
        output.push('<');
        output.push_str(&self.name);
        for (name, value) in &self.attributes {
            output.push(' ');
            output.push_str(name);
            output.push_str("=\"");
            escape_xml(value, output, true);
            output.push('"');
        }
        if self.children.is_empty() && self.text.is_empty() {
            output.push_str("/>");
            return;
        }
        output.push('>');
        escape_xml(&self.text, output, false);
        for child in &self.children {
            child.to_xml(output);
        }
        output.push_str("</");
        output.push_str(&self.name);
        output.push('>');
    }
}

pub(crate) fn parse(xml: &str) -> Result<XmlNode, String> {
    let mut parser = Parser {
        input: xml,
        offset: 0,
    };
    parser.skip_misc()?;
    if parser.starts_with("<!DOCTYPE") || parser.starts_with("<!doctype") {
        return Err(parser.error("DOCTYPE declarations are not supported"));
    }
    let root = parser.parse_element(0)?;
    parser.skip_misc()?;
    if parser.offset != parser.input.len() {
        return Err(parser.error("unexpected content after the document element"));
    }
    Ok(root)
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("XML parse error at byte {}: {message}", self.offset)
    }

    fn starts_with(&self, value: &str) -> bool {
        self.input[self.offset..].starts_with(value)
    }

    fn consume(&mut self, value: &str) -> bool {
        if !self.starts_with(value) {
            return false;
        }
        self.offset += value.len();
        true
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.input[self.offset..].chars().next() {
            if !character.is_whitespace() {
                break;
            }
            self.offset += character.len_utf8();
        }
    }

    fn skip_misc(&mut self) -> Result<(), String> {
        loop {
            self.skip_whitespace();
            if self.consume("<?") {
                self.skip_until("?>", "unterminated processing instruction")?;
            } else if self.consume("<!--") {
                self.skip_until("-->", "unterminated comment")?;
            } else {
                return Ok(());
            }
        }
    }

    fn skip_until(&mut self, terminator: &str, message: &str) -> Result<(), String> {
        let Some(relative_end) = self.input[self.offset..].find(terminator) else {
            return Err(self.error(message));
        };
        self.offset += relative_end + terminator.len();
        Ok(())
    }

    fn parse_element(&mut self, depth: usize) -> Result<XmlNode, String> {
        if depth >= 128 {
            return Err(self.error("element nesting exceeds 128 levels"));
        }
        if !self.consume("<") || self.starts_with("/") || self.starts_with("!") {
            return Err(self.error("expected an element start tag"));
        }
        let name = self.parse_name()?;
        let mut node = XmlNode::new(name);
        loop {
            self.skip_whitespace();
            if self.consume("/>") {
                return Ok(node);
            }
            if self.consume(">") {
                break;
            }
            let name = self.parse_name()?;
            self.skip_whitespace();
            if !self.consume("=") {
                return Err(self.error("expected '=' after an attribute name"));
            }
            self.skip_whitespace();
            let value = self.parse_attribute_value()?;
            if node.attribute(&name).is_some() {
                return Err(self.error("duplicate attribute"));
            }
            node.attributes.push((name, value));
        }

        loop {
            if self.offset >= self.input.len() {
                return Err(self.error("unterminated element"));
            }
            if self.consume("</") {
                let close_name = self.parse_name()?;
                self.skip_whitespace();
                if !self.consume(">") {
                    return Err(self.error("expected '>' after closing tag"));
                }
                if close_name != node.name {
                    return Err(self.error("closing tag does not match its element"));
                }
                return Ok(node);
            }
            if self.consume("<!--") {
                self.skip_until("-->", "unterminated comment")?;
            } else if self.consume("<![CDATA[") {
                let start = self.offset;
                let Some(relative_end) = self.input[start..].find("]]>") else {
                    return Err(self.error("unterminated CDATA section"));
                };
                node.text.push_str(&self.input[start..start + relative_end]);
                self.offset = start + relative_end + 3;
            } else if self.consume("<?") {
                self.skip_until("?>", "unterminated processing instruction")?;
            } else if self.starts_with("<!") {
                return Err(self.error("unsupported markup declaration"));
            } else if self.starts_with("<") {
                node.children.push(self.parse_element(depth + 1)?);
            } else {
                let start = self.offset;
                let end = self.input[start..]
                    .find('<')
                    .map_or(self.input.len(), |relative| start + relative);
                node.text
                    .push_str(&decode_entities(&self.input[start..end])?);
                self.offset = end;
            }
        }
    }

    fn parse_name(&mut self) -> Result<String, String> {
        let start = self.offset;
        while let Some(byte) = self.input.as_bytes().get(self.offset).copied() {
            if is_name_byte(byte, self.offset == start) {
                self.offset += 1;
            } else {
                break;
            }
        }
        if self.offset == start {
            return Err(self.error("expected an ASCII XML name"));
        }
        Ok(self.input[start..self.offset].to_owned())
    }

    fn parse_attribute_value(&mut self) -> Result<String, String> {
        let Some(quote) = self.input.as_bytes().get(self.offset).copied() else {
            return Err(self.error("expected a quoted attribute value"));
        };
        if quote != b'"' && quote != b'\'' {
            return Err(self.error("expected a quoted attribute value"));
        }
        self.offset += 1;
        let start = self.offset;
        let Some(relative_end) = self.input[start..].find(quote as char) else {
            return Err(self.error("unterminated attribute value"));
        };
        let value = decode_entities(&self.input[start..start + relative_end])?;
        self.offset = start + relative_end + 1;
        Ok(value)
    }
}

fn is_name_byte(byte: u8, first: bool) -> bool {
    byte.is_ascii_alphabetic()
        || byte == b'_'
        || byte == b':'
        || (!first && (byte.is_ascii_digit() || byte == b'-' || byte == b'.'))
}

fn decode_entities(value: &str) -> Result<String, String> {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(index) = remaining.find('&') {
        output.push_str(&remaining[..index]);
        remaining = &remaining[index + 1..];
        let Some(end) = remaining.find(';') else {
            return Err("XML entity is missing its terminating semicolon".to_owned());
        };
        let entity = &remaining[..end];
        match entity {
            "amp" => output.push('&'),
            "lt" => output.push('<'),
            "gt" => output.push('>'),
            "quot" => output.push('"'),
            "apos" => output.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                let code = u32::from_str_radix(&entity[2..], 16)
                    .map_err(|_| "invalid hexadecimal XML character reference".to_owned())?;
                output.push(
                    char::from_u32(code)
                        .ok_or_else(|| "invalid XML character reference".to_owned())?,
                );
            }
            _ if entity.starts_with('#') => {
                let code = entity[1..]
                    .parse::<u32>()
                    .map_err(|_| "invalid decimal XML character reference".to_owned())?;
                output.push(
                    char::from_u32(code)
                        .ok_or_else(|| "invalid XML character reference".to_owned())?,
                );
            }
            _ => return Err(format!("unsupported XML entity '&{entity};'")),
        }
        remaining = &remaining[end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn escape_xml(value: &str, output: &mut String, attribute: bool) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' if attribute => output.push_str("&quot;"),
            _ => {
                let _ = output.write_char(character);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, XmlNode};

    #[test]
    fn parses_and_writes_nested_nodes_with_escaped_text_and_attributes() {
        let root = parse(
            "<?xml version=\"1.0\"?><Root name=\"A &amp; B\"><Value><![CDATA[x < 2]]></Value></Root>",
        )
        .unwrap();
        assert_eq!(root.attribute("name"), Some("A & B"));
        assert_eq!(root.child("Value").unwrap().text, "x < 2");

        let mut output = String::new();
        root.to_xml(&mut output);
        assert_eq!(
            parse(&output).unwrap(),
            parse("<Root name=\"A &amp; B\"><Value>x &lt; 2</Value></Root>").unwrap()
        );
    }

    #[test]
    fn rejects_malformed_documents_unsupported_entities_and_doctypes() {
        assert!(parse("<A></B>").is_err());
        assert!(parse("<A value=\"&external;\"/>").is_err());
        assert!(parse("<!DOCTYPE A><A/>").is_err());
        assert!(parse("<A/><B/>").is_err());
    }

    #[test]
    fn writes_empty_nodes_and_updates_attributes_without_duplicates() {
        let mut node = XmlNode::new("Property");
        node.set_attribute("Name", "first");
        node.set_attribute("Name", "second");
        let mut output = String::new();
        node.to_xml(&mut output);
        assert_eq!(output, "<Property Name=\"second\"/>");
        assert_eq!(parse(&output).unwrap(), node);
    }
}
