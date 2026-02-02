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

        let (path, lca) = find_path(graph, &postorder, src, tgt);
        if path.is_empty() {
            continue;
        }

        if debug {
            let src_id = &graph.node_ids[src].0;
            let tgt_id = &graph.node_ids[tgt].0;
            let lca_label = lca
                .map(|l| graph.node_ids[l].0.clone())
                .unwrap_or_else(|| "None".to_string());
            let path_ids: Vec<String> = path.iter().map(|&p| graph.node_ids[p].0.clone()).collect();
            eprintln!(
                "[dummy_parents] edge {} src={} tgt={} lca={} path={:?}",
                chain.edge_index, src_id, tgt_id, lca_label, path_ids
            );
        }

        let mut path_idx = 0usize;
        let mut path_v = path[path_idx];
        // When lca is None (root-level), skip ascending entirely
        let mut ascending = lca.is_some();

        for dummy_id in &chain.dummy_ids {
            let Some(&dummy_idx) = graph.node_index.get(dummy_id) else {
                continue;
            };
            let dummy_rank = graph.ranks[dummy_idx];

            if ascending {
                while Some(path_v) != lca && max_rank(graph, path_v) < dummy_rank {
                    path_idx += 1;
                    if path_idx >= path.len() {
                        break;
                    }
                    path_v = path[path_idx];
                }
                if Some(path_v) == lca {
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
) -> (Vec<usize>, Option<usize>) {
    let low = postorder[v].low.min(postorder[w].low);
    let lim = postorder[v].lim.max(postorder[w].lim);

    // Traverse up from v to find the LCA.
    // Mirrors dagre's do-while: g.parent(v) can be undefined (None),
    // which is a valid LCA representing the root level.
    let mut v_path: Vec<usize> = Vec::new();
    let mut parent_opt = graph.parents[v];
    let lca: Option<usize> = loop {
        match parent_opt {
            Some(parent) => {
                v_path.push(parent);
                let range = &postorder[parent];
                if !(range.low > low || lim > range.lim) {
                    break Some(parent);
                }
                parent_opt = graph.parents[parent];
            }
            None => {
                // Reached root without finding an ancestor that spans both —
                // LCA is the implicit root (None).
                break None;
            }
        }
    };

    // Traverse from w up to the LCA.
    let mut w_path: Vec<usize> = Vec::new();
    let mut cur = w;
    loop {
        match graph.parents[cur] {
            Some(p) => {
                if Some(p) == lca {
                    break;
                }
                w_path.push(p);
                cur = p;
            }
            None => {
                // Reached root; if lca is also None, we're done
                break;
            }
        }
    }

    w_path.reverse();

    // For root-level LCA, v_path may contain ancestors that aren't the LCA itself,
    // so we only keep the w_path (descending from root into the target's hierarchy).
    if lca.is_none() {
        // v_path entries aren't useful (they were traversed but no LCA compound was found)
        return (w_path, None);
    }

    v_path.extend(w_path);
    (v_path, lca)
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::dagre::graph::DiGraph;
    use crate::dagre::{self, LayoutConfig, normalize, rank};

    /// Build a LayoutGraph matching the external_node_subgraph fixture and run
    /// the pipeline up through parent_dummy_chains.
    fn build_external_node_subgraph_after_parent_dummy_chains() -> LayoutGraph {
        // external_node_subgraph.mmd:
        //   graph TD
        //     subgraph Cloud
        //       subgraph us-east [US East Region]
        //         A[Web Server] --> B[App Server]
        //       end
        //       subgraph us-west [US West Region]
        //         C[Web Server] --> D[App Server]
        //       end
        //     end
        //     E[Load Balancer] --> A
        //     E --> C
        //
        // Node dimensions: "Web Server" = 14, "App Server" = 14, "Load Balancer" = 17
        // node_dimensions = label.len() + 4 => Web Server=14, App Server=14, Load Balancer=17
        // height = 3 for all
        let mut g: DiGraph<(usize, usize)> = DiGraph::new();

        // Add nodes in edge-first order (matching render/layout.rs behavior)
        g.add_node("A", (14, 3)); // Web Server
        g.add_node("B", (14, 3)); // App Server
        g.add_node("C", (14, 3)); // Web Server
        g.add_node("D", (14, 3)); // App Server
        g.add_node("E", (17, 3)); // Load Balancer (13 chars + 4 = 17)

        // Subgraph compound nodes (zero dimensions, sorted alphabetically like layout.rs)
        g.add_node("Cloud", (0, 0));
        g.add_node("us-east", (0, 0));
        g.add_node("us-west", (0, 0));

        // Titles
        g.set_has_title("Cloud");
        g.set_has_title("us-east");
        g.set_has_title("us-west");

        // Parent relationships for nodes
        g.set_parent("A", "us-east");
        g.set_parent("B", "us-east");
        g.set_parent("C", "us-west");
        g.set_parent("D", "us-west");

        // Parent relationships for nested subgraphs
        g.set_parent("us-east", "Cloud");
        g.set_parent("us-west", "Cloud");

        // Edges: A→B (0), C→D (1), E→A (2), E→C (3)
        g.add_edge("A", "B");
        g.add_edge("C", "D");
        g.add_edge("E", "A");
        g.add_edge("E", "C");

        let mut lg = LayoutGraph::from_digraph(&g, |_, dims| (dims.0 as f64, dims.1 as f64));

        // Run pipeline up through parent_dummy_chains (matching layout_with_labels)
        dagre::extract_self_edges(&mut lg);
        crate::dagre::acyclic::run(&mut lg);
        dagre::make_space_for_edge_labels(&mut lg, &HashMap::new());
        crate::dagre::nesting::run(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::remove_empty_ranks(&mut lg);
        crate::dagre::nesting::cleanup(&mut lg);
        rank::normalize(&mut lg);
        crate::dagre::nesting::insert_title_nodes(&mut lg);
        rank::normalize(&mut lg);
        crate::dagre::nesting::assign_rank_minmax(&mut lg);
        normalize::run(&mut lg, &HashMap::new());
        run(&mut lg);

        lg
    }

    #[test]
    fn assigns_parents_for_external_chains() {
        let lg = build_external_node_subgraph_after_parent_dummy_chains();

        // Collect dummy parents by (edge_index, rank) -> parent name
        let mut parents: HashMap<(usize, i32), Option<String>> = HashMap::new();
        for chain in &lg.dummy_chains {
            for dummy_id in &chain.dummy_ids {
                let &dummy_idx = lg.node_index.get(dummy_id).unwrap();
                let rank = lg.ranks[dummy_idx];
                let parent_name = lg.parents[dummy_idx].map(|p| lg.node_ids[p].0.clone());
                parents.insert((chain.edge_index, rank), parent_name);
            }
        }

        // Edge 2: E→A — dummies should get: rank 1 = None, rank 2 = Cloud, rank 3 = us-east
        // (rank numbers may differ from research due to title nodes and normalization,
        // so we check by collecting all dummies for this edge and verifying the parent sequence)
        let mut e_to_a_parents: Vec<(i32, Option<String>)> = parents
            .iter()
            .filter(|((edge, _), _)| *edge == 2)
            .map(|((_, rank), parent)| (*rank, parent.clone()))
            .collect();
        e_to_a_parents.sort_by_key(|(r, _)| *r);

        // The key assertion: at least some dummies should have compound parents (Cloud, us-east)
        let has_cloud_parent = e_to_a_parents
            .iter()
            .any(|(_, p)| p.as_deref() == Some("Cloud"));
        let has_us_east_parent = e_to_a_parents
            .iter()
            .any(|(_, p)| p.as_deref() == Some("us-east"));
        assert!(
            has_cloud_parent,
            "E→A chain should have a dummy parented to Cloud, got: {:?}",
            e_to_a_parents
        );
        assert!(
            has_us_east_parent,
            "E→A chain should have a dummy parented to us-east, got: {:?}",
            e_to_a_parents
        );

        // Edge 3: E→C — dummies should get: rank 1 = None, rank 2 = Cloud, rank 3 = us-west
        let mut e_to_c_parents: Vec<(i32, Option<String>)> = parents
            .iter()
            .filter(|((edge, _), _)| *edge == 3)
            .map(|((_, rank), parent)| (*rank, parent.clone()))
            .collect();
        e_to_c_parents.sort_by_key(|(r, _)| *r);

        let has_cloud_parent_c = e_to_c_parents
            .iter()
            .any(|(_, p)| p.as_deref() == Some("Cloud"));
        let has_us_west_parent = e_to_c_parents
            .iter()
            .any(|(_, p)| p.as_deref() == Some("us-west"));
        assert!(
            has_cloud_parent_c,
            "E→C chain should have a dummy parented to Cloud, got: {:?}",
            e_to_c_parents
        );
        assert!(
            has_us_west_parent,
            "E→C chain should have a dummy parented to us-west, got: {:?}",
            e_to_c_parents
        );
    }
}
