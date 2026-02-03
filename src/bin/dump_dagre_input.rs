use std::{env, fs};

use mmdflux::dagre::LayoutConfig;
use mmdflux::render::node_dimensions;
use mmdflux::{Direction, build_diagram, parse_flowchart};

fn json_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        eprintln!("Usage: dump_dagre_input <fixture-path>");
        std::process::exit(1);
    }

    let path = args.remove(0);
    let input = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", path, e);
        std::process::exit(1);
    });

    let flowchart = parse_flowchart(&input).unwrap_or_else(|e| {
        eprintln!("Failed to parse {}: {}", path, e);
        std::process::exit(1);
    });
    let diagram = build_diagram(&flowchart);

    let rankdir = match diagram.direction {
        Direction::TopDown => "TB",
        Direction::BottomTop => "BT",
        Direction::LeftRight => "LR",
        Direction::RightLeft => "RL",
    };

    // Match render/layout.rs dagre config defaults
    let (node_sep, edge_sep) = match diagram.direction {
        Direction::LeftRight | Direction::RightLeft => {
            let total_height: f64 = diagram
                .nodes
                .values()
                .map(|node| node_dimensions(node).1 as f64)
                .sum();
            let count = diagram.nodes.len().max(1) as f64;
            let avg_height = total_height / count;
            let ns = (avg_height * 2.0).max(6.0);
            let es = (avg_height * 0.8).max(2.0);
            (ns, es)
        }
        _ => (50.0, 20.0),
    };

    let ranksep = 50.0;
    let margin = LayoutConfig::default().margin;

    // Collect nodes (diagram nodes + subgraphs), sorted by id for determinism
    let mut nodes: Vec<(String, String, f64, f64, Option<String>, bool)> = Vec::new();
    for (id, node) in &diagram.nodes {
        let (w, h) = node_dimensions(node);
        nodes.push((
            id.clone(),
            node.label.clone(),
            w as f64,
            h as f64,
            node.parent.clone(),
            false,
        ));
    }
    for (id, sg) in &diagram.subgraphs {
        nodes.push((
            id.clone(),
            sg.title.clone(),
            0.0,
            0.0,
            sg.parent.clone(),
            true,
        ));
    }
    nodes.sort_by(|a, b| a.0.cmp(&b.0));

    // Collect edges
    let mut edges: Vec<(String, String, Option<String>, usize)> = Vec::new();
    for (idx, edge) in diagram.edges.iter().enumerate() {
        edges.push((edge.from.clone(), edge.to.clone(), edge.label.clone(), idx));
    }

    println!("{{");
    println!("  \"graph\": {{");
    println!("    \"rankdir\": \"{}\",", rankdir);
    println!("    \"nodesep\": {},", node_sep);
    println!("    \"edgesep\": {},", edge_sep);
    println!("    \"ranksep\": {},", ranksep);
    println!("    \"marginx\": {},", margin);
    println!("    \"marginy\": {},", margin);
    println!("    \"ranker\": \"network-simplex\"");
    println!("  }},");

    println!("  \"nodes\": [");
    for (i, (id, label, w, h, parent, is_subgraph)) in nodes.iter().enumerate() {
        let parent_json = match parent {
            Some(p) => format!("\"{}\"", json_escape(p)),
            None => "null".to_string(),
        };
        let suffix = if i + 1 == nodes.len() { "" } else { "," };
        println!(
            "    {{\"id\": \"{}\", \"label\": \"{}\", \"width\": {}, \"height\": {}, \"parent\": {}, \"is_subgraph\": {}}}{}",
            json_escape(id),
            json_escape(label),
            w,
            h,
            parent_json,
            if *is_subgraph { "true" } else { "false" },
            suffix
        );
    }
    println!("  ],");

    println!("  \"edges\": [");
    for (i, (from, to, label, idx)) in edges.iter().enumerate() {
        let (label_json, label_w, label_h) = match label {
            Some(l) => (
                format!("\"{}\"", json_escape(l)),
                l.chars().count() + 2,
                1usize,
            ),
            None => ("null".to_string(), 0usize, 0usize),
        };
        let suffix = if i + 1 == edges.len() { "" } else { "," };
        println!(
            "    {{\"from\": \"{}\", \"to\": \"{}\", \"label\": {}, \"label_width\": {}, \"label_height\": {}, \"index\": {}}}{}",
            json_escape(from),
            json_escape(to),
            label_json,
            label_w,
            label_h,
            idx,
            suffix
        );
    }
    println!("  ]");
    println!("}}");
}
