//! Canonical S-expression wire AST.
//!
//! Nodes are definitions and `(ref "node" "socket")` forms are references, so one
//! output may feed any number of consumers without expanding the source expression.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use super::{
    ComputePipelineDeclaration, ExecutorRef, Graph, GraphError, Node, NodeOutputRef, NodeState,
    PipelineDeclarations, RenderPipelineDeclaration, MAX_AST_BYTES,
};

const AST_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
enum SExpr {
    List(Vec<SExpr>),
    Atom(String),
    String(String),
}

struct Parser<'a> {
    source: &'a str,
    offset: usize,
}

impl Parser<'_> {
    fn skip_trivia(&mut self) {
        loop {
            while self
                .source
                .as_bytes()
                .get(self.offset)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.offset += 1;
            }
            if self.source.as_bytes().get(self.offset) != Some(&b';') {
                return;
            }
            while self
                .source
                .as_bytes()
                .get(self.offset)
                .is_some_and(|byte| *byte != b'\n')
            {
                self.offset += 1;
            }
        }
    }

    fn expression(&mut self) -> Result<SExpr, GraphError> {
        self.skip_trivia();
        match self.source.as_bytes().get(self.offset).copied() {
            Some(b'(') => self.list(),
            Some(b'"') => self.string(),
            Some(b')') | None => Err(invalid("expected expression", self.offset)),
            Some(_) => self.atom(),
        }
    }

    fn list(&mut self) -> Result<SExpr, GraphError> {
        self.offset += 1;
        let mut values = Vec::new();
        loop {
            self.skip_trivia();
            match self.source.as_bytes().get(self.offset).copied() {
                Some(b')') => {
                    self.offset += 1;
                    return Ok(SExpr::List(values));
                }
                None => return Err(invalid("unterminated list", self.offset)),
                _ => values.push(self.expression()?),
            }
        }
    }

    fn string(&mut self) -> Result<SExpr, GraphError> {
        let start = self.offset;
        self.offset += 1;
        let mut escaped = false;
        while let Some(byte) = self.source.as_bytes().get(self.offset).copied() {
            self.offset += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                let encoded = &self.source[start..self.offset];
                let value = serde_json::from_str(encoded)
                    .map_err(|_| invalid("invalid string literal", start))?;
                return Ok(SExpr::String(value));
            }
        }
        Err(invalid("unterminated string", start))
    }

    fn atom(&mut self) -> Result<SExpr, GraphError> {
        let start = self.offset;
        while self.source.as_bytes().get(self.offset).is_some_and(|byte| {
            !byte.is_ascii_whitespace() && !matches!(*byte, b'(' | b')' | b'"' | b';')
        }) {
            self.offset += 1;
        }
        if start == self.offset {
            Err(invalid("invalid token", start))
        } else {
            Ok(SExpr::Atom(self.source[start..self.offset].to_owned()))
        }
    }
}

fn invalid(message: impl Into<String>, offset: usize) -> GraphError {
    let message = message.into();
    GraphError {
        code: "GRAPH_AST_INVALID",
        message: message.clone(),
        details: serde_json::json!({"message":message,"offset":offset}),
    }
}

fn list(value: &SExpr) -> Result<&[SExpr], GraphError> {
    match value {
        SExpr::List(values) => Ok(values),
        _ => Err(invalid("expected list", 0)),
    }
}

fn atom(value: &SExpr) -> Result<&str, GraphError> {
    match value {
        SExpr::Atom(value) => Ok(value),
        _ => Err(invalid("expected symbol", 0)),
    }
}

fn string(value: &SExpr) -> Result<String, GraphError> {
    match value {
        SExpr::String(value) => Ok(value.clone()),
        _ => Err(invalid("expected string", 0)),
    }
}

fn u32_value(value: &SExpr) -> Result<u32, GraphError> {
    atom(value)?.parse().map_err(|_| invalid("expected u32", 0))
}

fn named_fields<'a>(values: &'a [SExpr]) -> Result<BTreeMap<&'a str, &'a [SExpr]>, GraphError> {
    let mut fields = BTreeMap::new();
    for value in values {
        let field = list(value)?;
        let Some(name) = field.first() else {
            return Err(invalid("empty field", 0));
        };
        let name = atom(name)?;
        if fields.insert(name, &field[1..]).is_some() {
            return Err(invalid(format!("duplicate field '{name}'"), 0));
        }
    }
    Ok(fields)
}

fn exact_field<'a>(
    fields: &BTreeMap<&str, &'a [SExpr]>,
    name: &str,
    length: usize,
) -> Result<&'a [SExpr], GraphError> {
    let values = fields
        .get(name)
        .copied()
        .ok_or_else(|| invalid(format!("missing field '{name}'"), 0))?;
    if values.len() != length {
        return Err(invalid(format!("field '{name}' has invalid arity"), 0));
    }
    Ok(values)
}

fn json_value(value: &SExpr) -> Result<Value, GraphError> {
    match value {
        SExpr::String(value) => Ok(Value::String(value.clone())),
        SExpr::Atom(value) if value == "true" => Ok(Value::Bool(true)),
        SExpr::Atom(value) if value == "false" => Ok(Value::Bool(false)),
        SExpr::Atom(value) if value == "null" => Ok(Value::Null),
        SExpr::Atom(value) => serde_json::from_str(value)
            .map_err(|_| invalid("value atom must be a finite JSON number", 0)),
        SExpr::List(values)
            if values.first().and_then(|value| atom(value).ok()) == Some("array") =>
        {
            values[1..]
                .iter()
                .map(json_value)
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        SExpr::List(values)
            if values.first().and_then(|value| atom(value).ok()) == Some("object") =>
        {
            let mut object = serde_json::Map::new();
            for field in &values[1..] {
                let field = list(field)?;
                if field.len() != 3 || atom(&field[0])? != "field" {
                    return Err(invalid("object entries must be (field string value)", 0));
                }
                let key = string(&field[1])?;
                if object.insert(key.clone(), json_value(&field[2])?).is_some() {
                    return Err(invalid(format!("duplicate object field '{key}'"), 0));
                }
            }
            Ok(Value::Object(object))
        }
        _ => Err(invalid("invalid data value", 0)),
    }
}

fn node(value: &SExpr) -> Result<Node, GraphError> {
    let values = list(value)?;
    if values.len() < 4 || atom(&values[0])? != "node" {
        return Err(invalid("invalid node definition", 0));
    }
    let id = string(&values[1])?;
    let state = match atom(&values[2])? {
        "enabled" => NodeState::Enabled,
        "muted" => NodeState::Muted,
        _ => return Err(invalid("node state must be enabled or muted", 0)),
    };
    let fields = named_fields(&values[3..])?;
    if fields.len() != 3 {
        return Err(invalid("node requires executor, params, and inputs", 0));
    }
    let executor = exact_field(&fields, "executor", 2)?;
    let parameters = json_value(&exact_field(&fields, "params", 1)?[0])?;
    let input_forms = fields
        .get("inputs")
        .copied()
        .ok_or_else(|| invalid("missing field 'inputs'", 0))?;
    let mut inputs = BTreeMap::new();
    for input in input_forms {
        let input = list(input)?;
        if input.len() < 2 || atom(&input[0])? != "input" {
            return Err(invalid("invalid input definition", 0));
        }
        let name = string(&input[1])?;
        let mut references = Vec::new();
        for reference in &input[2..] {
            let reference = list(reference)?;
            if reference.len() != 3 || atom(&reference[0])? != "ref" {
                return Err(invalid("invalid DAG reference", 0));
            }
            references.push(NodeOutputRef {
                node: string(&reference[1])?,
                socket: string(&reference[2])?,
            });
        }
        if inputs.insert(name.clone(), references).is_some() {
            return Err(invalid(format!("duplicate input '{name}'"), 0));
        }
    }
    Ok(Node {
        id,
        state,
        executor: ExecutorRef {
            key: string(&executor[0])?,
            version: u32_value(&executor[1])?,
        },
        parameters,
        inputs,
    })
}

/// Parses the only render-graph wire format accepted by Yawn core.
pub fn parse(bytes: &[u8]) -> Result<Graph, GraphError> {
    if bytes.len() > MAX_AST_BYTES {
        return Err(GraphError::new(
            "GRAPH_PAYLOAD_TOO_LARGE",
            "graph AST exceeds 1 MiB",
        ));
    }
    let source = std::str::from_utf8(bytes)
        .map_err(|_| GraphError::new("GRAPH_ENCODING_INVALID", "graph AST is not UTF-8"))?;
    let mut parser = Parser { source, offset: 0 };
    let root = parser.expression()?;
    parser.skip_trivia();
    if parser.offset != source.len() {
        return Err(invalid("trailing expression", parser.offset));
    }
    let root = list(&root)?;
    if root.len() < 2 || atom(&root[0])? != "yawn-graph" {
        return Err(invalid("root must be yawn-graph", 0));
    }
    if u32_value(&root[1])? != AST_VERSION {
        return Err(GraphError::new(
            "GRAPH_SCHEMA_UNSUPPORTED",
            "render graph AST version must be 1",
        ));
    }
    let fields = named_fields(&root[2..])?;
    if fields.len() != 4 {
        return Err(invalid(
            "graph requires id, revision, pipelines, and nodes",
            0,
        ));
    }
    let graph_id = string(&exact_field(&fields, "id", 1)?[0])?;
    let revision = u32_value(&exact_field(&fields, "revision", 1)?[0])?;
    let pipelines_value = json_value(&exact_field(&fields, "pipelines", 1)?[0])?;
    let pipelines: PipelineDeclarations = serde_json::from_value(pipelines_value)
        .map_err(|error| invalid(format!("invalid pipeline declarations: {error}"), 0))?;
    let nodes = fields
        .get("nodes")
        .copied()
        .ok_or_else(|| invalid("missing field 'nodes'", 0))?
        .iter()
        .map(node)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Graph {
        schema_version: 3,
        graph_id,
        revision,
        pipelines,
        nodes,
    })
}

#[cfg(test)]
fn push_string(out: &mut String, value: &str) {
    out.push_str(&serde_json::to_string(value).expect("strings always serialize"));
}

#[cfg(test)]
fn push_json(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => out.push_str(&value.to_string()),
        Value::String(value) => push_string(out, value),
        Value::Array(values) => {
            out.push_str("(array");
            for value in values {
                out.push(' ');
                push_json(out, value);
            }
            out.push(')');
        }
        Value::Object(values) => {
            out.push_str("(object");
            let mut fields: Vec<_> = values.iter().collect();
            fields.sort_by(|left, right| left.0.cmp(right.0));
            for (name, value) in fields {
                out.push_str(" (field ");
                push_string(out, name);
                out.push(' ');
                push_json(out, value);
                out.push(')');
            }
            out.push(')');
        }
    }
}

/// Serializes an internal graph for fixtures and cross-language conformance tests.
#[cfg(test)]
pub fn serialize(graph: &Graph) -> String {
    let mut out = format!("(yawn-graph {AST_VERSION}\n  (id ");
    push_string(&mut out, &graph.graph_id);
    out.push_str(&format!(
        ")\n  (revision {})\n  (pipelines ",
        graph.revision
    ));
    push_json(
        &mut out,
        &serde_json::to_value(&graph.pipelines).expect("pipeline declarations serialize"),
    );
    out.push_str(")\n  (nodes");
    for node in &graph.nodes {
        out.push_str("\n    (node ");
        push_string(&mut out, &node.id);
        out.push(' ');
        out.push_str(match node.state {
            NodeState::Enabled => "enabled",
            NodeState::Muted => "muted",
        });
        out.push_str("\n      (executor ");
        push_string(&mut out, &node.executor.key);
        out.push_str(&format!(" {})\n      (params ", node.executor.version));
        push_json(&mut out, &node.parameters);
        out.push_str(")\n      (inputs");
        for (name, references) in &node.inputs {
            out.push_str("\n        (input ");
            push_string(&mut out, name);
            for reference in references {
                out.push_str(" (ref ");
                push_string(&mut out, &reference.node);
                out.push(' ');
                push_string(&mut out, &reference.socket);
                out.push(')');
            }
            out.push(')');
        }
        out.push_str(")\n    )");
    }
    out.push_str("))\n");
    out
}

pub(crate) fn validate_pipeline_declarations(graph: &Graph) -> Result<(), GraphError> {
    let mut names = HashSet::new();
    let mut shader_bytes = 0usize;
    for RenderPipelineDeclaration {
        name,
        shader,
        vertex_entry,
        fragment_entry,
        ..
    } in &graph.pipelines.render
    {
        if !names.insert(name) {
            return Err(GraphError::new(
                "GRAPH_DUPLICATE_ID",
                format!("duplicate authored pipeline '{name}'"),
            ));
        }
        for identifier in [name, vertex_entry, fragment_entry] {
            if !super::identifier(identifier) || identifier.len() > 64 {
                return Err(GraphError::new(
                    "GRAPH_INVALID_ID",
                    "invalid authored render pipeline identifier",
                ));
            }
        }
        if super::contract(name).is_some_and(|contract| {
            !contract.is_raster_draw()
                && contract.fullscreen_policy.is_none()
                && name != "frame_out"
        }) {
            return Err(GraphError::new(
                "GRAPH_EXECUTION_UNSUPPORTED",
                format!("authored render pipeline '{name}' conflicts with a core executor"),
            ));
        }
        shader_bytes = shader_bytes.saturating_add(shader.len());
    }
    for ComputePipelineDeclaration {
        name,
        shader,
        entry,
        dispatch,
    } in &graph.pipelines.compute
    {
        if !names.insert(name) {
            return Err(GraphError::new(
                "GRAPH_DUPLICATE_ID",
                format!("duplicate authored pipeline '{name}'"),
            ));
        }
        for identifier in [name, entry] {
            if !super::identifier(identifier) || identifier.len() > 64 {
                return Err(GraphError::new(
                    "GRAPH_INVALID_ID",
                    "invalid authored compute pipeline identifier",
                ));
            }
        }
        if dispatch.contains(&0) {
            return Err(GraphError::new(
                "GRAPH_PARAMETERS_INVALID",
                "compute dispatch dimensions must be nonzero",
            ));
        }
        shader_bytes = shader_bytes.saturating_add(shader.len());
    }
    if shader_bytes > MAX_AST_BYTES / 2 {
        return Err(GraphError::new(
            "GRAPH_LIMIT_EXCEEDED",
            "authored shader source exceeds 512 KiB",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_round_trip_preserves_shared_dag_references() {
        let graph = Graph {
            schema_version: 3,
            graph_id: "dag".into(),
            revision: 7,
            pipelines: PipelineDeclarations::default(),
            nodes: vec![Node {
                id: "consumer".into(),
                state: NodeState::Enabled,
                executor: ExecutorRef {
                    key: "and".into(),
                    version: 2,
                },
                parameters: serde_json::json!({}),
                inputs: BTreeMap::from([(
                    "inputs".into(),
                    vec![
                        NodeOutputRef {
                            node: "shared".into(),
                            socket: "value".into(),
                        },
                        NodeOutputRef {
                            node: "shared".into(),
                            socket: "value".into(),
                        },
                    ],
                )]),
            }],
        };
        let encoded = serialize(&graph);
        let decoded = parse(encoded.as_bytes()).unwrap();
        assert_eq!(decoded.graph_id, "dag");
        assert_eq!(decoded.nodes[0].inputs["inputs"].len(), 2);
        assert_eq!(serialize(&decoded), encoded);
    }

    #[test]
    fn rejects_duplicate_fields_and_trailing_expressions() {
        let duplicate = b"(yawn-graph 1 (id \"x\") (id \"y\") (revision 1) (pipelines (object (field \"render\" (array)) (field \"compute\" (array)))) (nodes))";
        assert_eq!(parse(duplicate).unwrap_err().code, "GRAPH_AST_INVALID");

        let trailing = b"(yawn-graph 1 (id \"x\") (revision 1) (pipelines (object (field \"render\" (array)) (field \"compute\" (array)))) (nodes)) true";
        assert_eq!(parse(trailing).unwrap_err().code, "GRAPH_AST_INVALID");
    }

    #[test]
    fn rejects_json_and_unknown_top_level_fields() {
        assert_eq!(
            parse(br#"{"graphId":"old"}"#).unwrap_err().code,
            "GRAPH_AST_INVALID"
        );
        let source = b"(yawn-graph 1 (id \"x\") (revision 1) (pipelines (object (field \"render\" (array)) (field \"compute\" (array)))) (nodes) (legacy true))";
        assert_eq!(parse(source).unwrap_err().code, "GRAPH_AST_INVALID");
    }
}
