//! Topological order of instances from bindings (DAG).

use std::collections::{HashMap, HashSet, VecDeque};
use wvx_ir::Project;

pub fn topo_order(project: &Project) -> Result<Vec<String>, String> {
    let ids: HashSet<String> = project.instances.iter().map(|i| i.id.clone()).collect();
    let mut indegree: HashMap<String, usize> = ids.iter().map(|id| (id.clone(), 0)).collect();
    let mut outgoing: HashMap<String, Vec<String>> =
        ids.iter().map(|id| (id.clone(), Vec::new())).collect();

    for binding in &project.bindings {
        let from = &binding.from.instance;
        let to = &binding.to.instance;
        if from == to {
            continue;
        }
        if !ids.contains(from) || !ids.contains(to) {
            continue;
        }
        outgoing.get_mut(from).unwrap().push(to.clone());
        *indegree.get_mut(to).unwrap() += 1;
    }

    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| id.clone())
        .collect();
    queue.make_contiguous().sort();

    let mut order = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id.clone());
        let mut nexts = outgoing.remove(&id).unwrap_or_default();
        nexts.sort();
        for n in nexts {
            let e = indegree.get_mut(&n).unwrap();
            *e -= 1;
            if *e == 0 {
                queue.push_back(n);
            }
        }
    }

    if order.len() != ids.len() {
        return Err("project graph contains a cycle".into());
    }
    Ok(order)
}
