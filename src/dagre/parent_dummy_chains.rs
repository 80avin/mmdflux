//! Assign parents to dummy chains so they track compound hierarchy.
//!
//! Mirrors dagre's parent-dummy-chains.js. This ensures dummy nodes created
//! during normalization are associated with the correct compound ancestors,
//! which affects ordering and border placement.

use super::graph::LayoutGraph;

#[derive(Clone, Copy, Debug)]
struct PostorderRange {
    low: i32,
    lim: i32,
}

pub(crate) fn run(graph: &mut LayoutGraph) {
    if graph.dummy_chains.is_empty() {
        return;
    }

    let debug = std::env::var("MMDFLUX_DEBUG_DUMMY_PARENTS").is_ok_and(|v| v == "1");
    let postorder = compute_postorder(graph);

    for chain in &graph.dummy_chains {
        let Some((src, tgt)) = find_original_edge_endpoints(graph, chain.edge_index) else {
            continue;
        };

        let Some((path, lca)) = find_path(graph, &postorder, src, tgt) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }

        if debug {
            let src_id = &graph.node_ids[src].0;
            let tgt_id = &graph.node_ids[tgt].0;
            let lca_id = &graph.node_ids[lca].0;
            let path_ids: Vec<String> = path.iter().map(|&p| graph.node_ids[p].0.clone()).collect();
            eprintln!(
                "[dummy_parents] edge {} src={} tgt={} lca={} path={:?}",
                chain.edge_index, src_id, tgt_id, lca_id, path_ids
            );
        }

        let mut path_idx = 0usize;
        let mut path_v = path[path_idx];
        let mut ascending = true;

        for dummy_id in &chain.dummy_ids {
            let Some(&dummy_idx) = graph.node_index.get(dummy_id) else {
                continue;
            };
            let dummy_rank = graph.ranks[dummy_idx];

            if ascending {
                while path_v != lca && max_rank(graph, path_v) < dummy_rank {
                    path_idx += 1;
                    if path_idx >= path.len() {
                        break;
                    }
                    path_v = path[path_idx];
                }
                if path_v == lca {
                    ascending = false;
                }
            }

            if !ascending {
                while path_idx + 1 < path.len() && min_rank(graph, path[path_idx + 1]) <= dummy_rank
                {
                    path_idx += 1;
                }
                path_v = path[path_idx];
            }

            graph.parents[dummy_idx] = Some(path_v);
            if debug {
                let dummy_name = &graph.node_ids[dummy_idx].0;
                let parent_name = &graph.node_ids[path_v].0;
                eprintln!(
                    "[dummy_parents]   dummy {} rank {} -> {}",
                    dummy_name, dummy_rank, parent_name
                );
            }
        }
    }
}

fn compute_postorder(graph: &LayoutGraph) -> Vec<PostorderRange> {
    let n = graph.node_ids.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (child, parent) in graph.parents.iter().enumerate() {
        if let Some(p) = parent {
            children[*p].push(child);
        }
    }
    for kids in &mut children {
        kids.sort();
    }

    let mut result = vec![PostorderRange { low: 0, lim: 0 }; n];
    let mut lim: i32 = 0;

    fn dfs(v: usize, children: &[Vec<usize>], result: &mut [PostorderRange], lim: &mut i32) {
        let low = *lim;
        for &child in &children[v] {
            dfs(child, children, result, lim);
        }
        result[v] = PostorderRange { low, lim: *lim };
        *lim += 1;
    }

    for v in 0..n {
        if graph.parents[v].is_none() {
            dfs(v, &children, &mut result, &mut lim);
        }
    }

    result
}

fn find_path(
    graph: &LayoutGraph,
    postorder: &[PostorderRange],
    v: usize,
    w: usize,
) -> Option<(Vec<usize>, usize)> {
    let low = postorder[v].low.min(postorder[w].low);
    let lim = postorder[v].lim.max(postorder[w].lim);

    let mut v_path: Vec<usize> = Vec::new();
    let mut parent = v;
    loop {
        parent = graph.parents[parent]?;
        v_path.push(parent);
        let range = postorder.get(parent)?;
        if !(range.low > low || lim > range.lim) {
            break;
        }
    }
    let lca = parent;

    let mut w_path: Vec<usize> = Vec::new();
    let mut cur = w;
    while let Some(p) = graph.parents[cur] {
        cur = p;
        if cur == lca {
            break;
        }
        w_path.push(cur);
    }
    if cur != lca {
        return None;
    }
    w_path.reverse();
    v_path.extend(w_path);
    Some((v_path, lca))
}

fn min_rank(graph: &LayoutGraph, node: usize) -> i32 {
    graph
        .min_rank
        .get(&node)
        .copied()
        .unwrap_or(graph.ranks[node])
}

fn max_rank(graph: &LayoutGraph, node: usize) -> i32 {
    graph
        .max_rank
        .get(&node)
        .copied()
        .unwrap_or(graph.ranks[node])
}

fn find_original_edge_endpoints(graph: &LayoutGraph, orig_idx: usize) -> Option<(usize, usize)> {
    graph.original_edge_endpoints.get(orig_idx).copied()
}
