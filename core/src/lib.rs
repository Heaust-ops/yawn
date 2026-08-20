use std::collections::{HashMap, HashSet};

use serde_json::{Map, Number, Value};
use wasm_bindgen::prelude::*;

const ALIGNMENT: u32 = 64;

#[wasm_bindgen]
pub struct Core {
    _arena: Box<[u8]>,
    base: u32,
    capacity: u32,
    used: u32,
}

#[wasm_bindgen]
impl Core {
    #[wasm_bindgen(constructor)]
    pub fn new(arena_bytes: u32) -> Result<Self, JsError> {
        if arena_bytes < ALIGNMENT || arena_bytes > u32::MAX - (ALIGNMENT - 1) {
            return Err(JsError::new("INIT"));
        }
        let mut arena = vec![0; (arena_bytes + ALIGNMENT - 1) as usize].into_boxed_slice();
        let pointer = arena.as_mut_ptr() as usize;
        let base = ((pointer + ALIGNMENT as usize - 1) & !(ALIGNMENT as usize - 1)) as u32;
        Ok(Self {
            _arena: arena,
            base,
            capacity: arena_bytes,
            used: 0,
        })
    }

    pub fn allocate(&mut self, rows: u32, stride: u32, format: &str) -> Result<u32, JsError> {
        if rows == 0 || stride < 16 || stride % 16 != 0 || !matches!(format, "f32" | "u32" | "i32")
        {
            return Err(JsError::new("ALLOCATION"));
        }
        let offset = align(self.used, ALIGNMENT).ok_or_else(|| JsError::new("ARENA_OOM"))?;
        let bytes = rows
            .checked_mul(stride)
            .ok_or_else(|| JsError::new("ARENA_OOM"))?;
        self.used = offset
            .checked_add(bytes)
            .filter(|end| *end <= self.capacity)
            .ok_or_else(|| JsError::new("ARENA_OOM"))?;
        Ok(self.base + offset)
    }

    pub fn compile_graph(&self, source: &str) -> Result<String, JsError> {
        compile_graph(source).map_err(JsError::new)
    }
}

fn align(value: u32, alignment: u32) -> Option<u32> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
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
        let mut parser = Parser {
            source: source.as_bytes(),
            at: 0,
        };
        let expression = parser.expression()?;
        parser.whitespace();
        if parser.at != parser.source.len() {
            return Err("GRAPH_WIRE");
        }
        Ok(expression)
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

fn compile_graph(source: &str) -> Result<String, &'static str> {
    let root = Parser::parse(source)?;
    let Expression::List(mut root) = root else {
        return Err("GRAPH_WIRE");
    };
    if root.len() != 3
        || !matches!(&root[0], Expression::Atom(Value::String(tag)) if tag == "yawn-graph")
        || !matches!(&root[1], Expression::Atom(Value::Number(version)) if version.as_u64() == Some(1))
    {
        return Err("GRAPH_WIRE");
    }
    let mut graph = decode(root.pop().unwrap())?;
    let object = graph.as_object_mut().ok_or("GRAPH_SHAPE")?;
    if !object.get("id").is_some_and(Value::is_string) {
        return Err("GRAPH_SHAPE");
    }
    let passes = sort_passes(object.get("passes").ok_or("GRAPH_PASS")?)?;
    plan_resources(object, &passes)?;
    object.insert("passes".into(), Value::Array(passes));
    serde_json::to_string(&graph).map_err(|_| "GRAPH_WIRE")
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

fn sort_passes(value: &Value) -> Result<Vec<Value>, &'static str> {
    let passes = value.as_array().ok_or("GRAPH_PASS")?;
    let mut ids = HashMap::new();
    for (index, pass) in passes.iter().enumerate() {
        let id = pass
            .as_object()
            .and_then(|pass| pass.get("id"))
            .and_then(Value::as_str)
            .ok_or("GRAPH_PASS")?;
        if ids.insert(id, index).is_some() {
            return Err("GRAPH_PASS");
        }
    }
    let mut dependencies = Vec::with_capacity(passes.len());
    for pass in passes {
        let after = pass.get("after").map_or(Ok(&[][..]), |after| {
            after.as_array().map(Vec::as_slice).ok_or("GRAPH_ARRAY")
        })?;
        dependencies.push(
            after
                .iter()
                .map(|dependency| {
                    dependency
                        .as_str()
                        .and_then(|id| ids.get(id).copied())
                        .ok_or("GRAPH_DEPENDENCY")
                })
                .collect::<Result<HashSet<_>, _>>()?,
        );
    }
    let mut emitted = vec![false; passes.len()];
    let mut result = Vec::with_capacity(passes.len());
    while result.len() < passes.len() {
        let ready = (0..passes.len()).find(|&index| {
            !emitted[index]
                && dependencies[index]
                    .iter()
                    .all(|dependency| emitted[*dependency])
        });
        let index = ready.ok_or("GRAPH_CYCLE")?;
        emitted[index] = true;
        result.push(passes[index].clone());
    }
    Ok(result)
}

fn plan_resources(graph: &mut Map<String, Value>, passes: &[Value]) -> Result<(), &'static str> {
    let mut used = HashSet::new();
    let mut lifetimes = HashMap::new();
    for (frame, pass) in passes.iter().enumerate() {
        for id in pass_resources(pass)? {
            used.insert(id.to_owned());
            lifetimes
                .entry(id.to_owned())
                .and_modify(|lifetime: &mut (usize, usize)| lifetime.1 = frame)
                .or_insert((frame, frame));
        }
    }
    let Some(resources) = graph.get_mut("resources") else {
        return Ok(());
    };
    let resources = resources.as_object_mut().ok_or("GRAPH_RESOURCE")?;
    for kind in ["buffers", "samplers"] {
        if let Some(declarations) = resources.get_mut(kind) {
            let declarations = declarations.as_array_mut().ok_or("GRAPH_RESOURCE")?;
            let mut ids = HashSet::new();
            declarations.retain(|declaration| {
                declaration
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| ids.insert(id.to_owned()) && used.contains(id))
            });
        }
    }
    let Some(textures) = resources.get_mut("textures") else {
        return Ok(());
    };
    let textures = textures.as_array_mut().ok_or("GRAPH_RESOURCE")?;
    let mut ids = HashSet::new();
    let mut planned = Vec::new();
    for mut declaration in textures.drain(..) {
        let object = declaration.as_object_mut().ok_or("GRAPH_RESOURCE")?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or("GRAPH_RESOURCE")?;
        if !ids.insert(id.to_owned()) {
            return Err("GRAPH_RESOURCE");
        }
        let Some(&(first, last)) = lifetimes.get(id) else {
            continue;
        };
        planned.push((declaration, first, last));
    }
    planned.sort_by_key(|value| value.1);
    let mut output = Vec::new();
    let mut slots: Vec<(String, usize)> = Vec::new();
    for (mut declaration, first, last) in planned {
        let object = declaration.as_object_mut().unwrap();
        let key = texture_key(object)?;
        let slot = if object.get("transient") == Some(&Value::Bool(false)) {
            None
        } else {
            slots
                .iter()
                .position(|value| value.0 == key && value.1 < first)
        };
        let slot = match slot {
            Some(slot) => {
                slots[slot].1 = last;
                slot
            }
            None => {
                slots.push((key, last));
                slots.len() - 1
            }
        };
        object.insert("slot".into(), Value::Number(Number::from(slot as u64)));
        output.push(declaration);
    }
    *textures = output;
    Ok(())
}

fn pass_resources(pass: &Value) -> Result<Vec<&str>, &'static str> {
    let mut result = Vec::new();
    for field in ["bindings", "color", "vertexBuffers"] {
        if let Some(values) = pass.get(field) {
            for value in values.as_array().ok_or("GRAPH_ARRAY")? {
                result.push(
                    value
                        .get("resource")
                        .and_then(Value::as_str)
                        .ok_or("GRAPH_RESOURCE")?,
                );
            }
        }
    }
    for field in ["depth", "indexBuffer"] {
        if let Some(value) = pass.get(field) {
            result.push(
                value
                    .get("resource")
                    .and_then(Value::as_str)
                    .ok_or("GRAPH_RESOURCE")?,
            );
        }
    }
    Ok(result)
}

fn texture_key(texture: &Map<String, Value>) -> Result<String, &'static str> {
    let mut size = texture.get("size").cloned().unwrap_or_else(|| {
        Value::Array(vec![
            Value::String("canvas".into()),
            Value::String("canvas".into()),
        ])
    });
    let size = size.as_array_mut().ok_or("GRAPH_TEXTURE_SIZE")?;
    if !(2..=3).contains(&size.len()) {
        return Err("GRAPH_TEXTURE_SIZE");
    }
    if size.len() == 2 {
        size.push(Value::Number(Number::from(1)));
    }
    let format = texture
        .get("format")
        .and_then(Value::as_str)
        .ok_or("GRAPH_RESOURCE")?;
    let mut usage = texture.get("usage").map_or(Ok(Vec::new()), |usage| {
        usage
            .as_array()
            .ok_or("GRAPH_TEXTURE_USAGE")?
            .iter()
            .map(|value| value.as_str().ok_or("GRAPH_TEXTURE_USAGE"))
            .collect::<Result<Vec<_>, _>>()
    })?;
    usage.sort_unstable();
    usage.dedup();
    serde_json::to_string(&(
        size,
        format,
        usage,
        texture
            .get("mipLevelCount")
            .and_then(Value::as_u64)
            .unwrap_or(1),
        texture
            .get("sampleCount")
            .and_then(Value::as_u64)
            .unwrap_or(1),
        texture
            .get("dimension")
            .and_then(Value::as_str)
            .unwrap_or("2d"),
    ))
    .map_err(|_| "GRAPH_RESOURCE")
}
