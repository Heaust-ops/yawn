use serde_json::{Map, Value};

use crate::graph::RenderGraph;

pub fn compile(source: &str) -> Result<RenderGraph, &'static str> {
    let Expression::List(mut root) = Parser::parse(source)? else {
        return Err("GRAPH_WIRE");
    };
    if root.len() != 3
        || !matches!(&root[0], Expression::Atom(Value::String(tag)) if tag == "yawn-graph")
        || !matches!(&root[1], Expression::Atom(Value::Number(version)) if version.as_u64() == Some(1))
    {
        return Err("GRAPH_WIRE");
    }
    let value = decode(root.pop().unwrap())?;
    let mut graph: RenderGraph = serde_json::from_value(value).map_err(|_| "GRAPH_SHAPE")?;
    graph.prepare()?;
    Ok(graph)
}

enum Expression {
    Atom(Value),
    List(Vec<Expression>),
}

struct Parser<'a> {
    source: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn parse(source: &'a str) -> Result<Expression, &'static str> {
        let mut parser = Self {
            source: source.as_bytes(),
            at: 0,
        };
        let expression = parser.expression()?;
        parser.whitespace();
        (parser.at == parser.source.len())
            .then_some(expression)
            .ok_or("GRAPH_WIRE")
    }

    fn expression(&mut self) -> Result<Expression, &'static str> {
        self.whitespace();
        match self.source.get(self.at) {
            Some(b'(') => self.list(),
            Some(b'"') => self.string(),
            Some(b')') | None => Err("GRAPH_WIRE"),
            Some(_) => self.atom(),
        }
    }

    fn list(&mut self) -> Result<Expression, &'static str> {
        self.at += 1;
        let mut values = Vec::new();
        loop {
            self.whitespace();
            match self.source.get(self.at) {
                Some(b')') => {
                    self.at += 1;
                    return Ok(Expression::List(values));
                }
                None => return Err("GRAPH_WIRE"),
                _ => values.push(self.expression()?),
            }
        }
    }

    fn string(&mut self) -> Result<Expression, &'static str> {
        let start = self.at;
        self.at += 1;
        let mut escaped = false;
        while let Some(&byte) = self.source.get(self.at) {
            self.at += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                let value = serde_json::from_slice(&self.source[start..self.at])
                    .map_err(|_| "GRAPH_WIRE")?;
                return Ok(Expression::Atom(Value::String(value)));
            }
        }
        Err("GRAPH_WIRE")
    }

    fn atom(&mut self) -> Result<Expression, &'static str> {
        let start = self.at;
        while self
            .source
            .get(self.at)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'(' | b')'))
        {
            self.at += 1;
        }
        let token = std::str::from_utf8(&self.source[start..self.at]).map_err(|_| "GRAPH_WIRE")?;
        let value = match token {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            _ => serde_json::from_str::<Value>(token)
                .ok()
                .filter(Value::is_number)
                .unwrap_or_else(|| Value::String(token.into())),
        };
        Ok(Expression::Atom(value))
    }

    fn whitespace(&mut self) {
        while self
            .source
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
}

fn decode(expression: Expression) -> Result<Value, &'static str> {
    let Expression::List(mut values) = expression else {
        return match expression {
            Expression::Atom(value) => Ok(value),
            Expression::List(_) => unreachable!(),
        };
    };
    if values.is_empty() {
        return Err("GRAPH_WIRE");
    }
    let tag = match values.remove(0) {
        Expression::Atom(Value::String(tag)) => tag,
        _ => return Err("GRAPH_WIRE"),
    };
    if tag == "array" {
        return values.into_iter().map(decode).collect();
    }
    if tag != "object" {
        return Err("GRAPH_WIRE");
    }
    let mut object = Map::new();
    for field in values {
        let Expression::List(mut field) = field else {
            return Err("GRAPH_WIRE");
        };
        if field.len() != 3
            || !matches!(&field[0], Expression::Atom(Value::String(tag)) if tag == "field")
        {
            return Err("GRAPH_WIRE");
        }
        let value = decode(field.pop().unwrap())?;
        let key = match field.pop().unwrap() {
            Expression::Atom(Value::String(key)) => key,
            _ => return Err("GRAPH_WIRE"),
        };
        if object.insert(key, value).is_some() {
            return Err("GRAPH_WIRE");
        }
    }
    Ok(Value::Object(object))
}
