use rustc_hash::FxHashMap;
use std::sync::Mutex;
use tsrs_core::P;
use tsrs_execute::build::buildtask::BuildTask;
use tsrs_execute::tsc::System;

use tsrs_execute::build::buildtask::UpstreamTask;
use tsrs_execute::build::orchestrator::*;

#[test]
fn schedules_full_selected_and_references_only_graphs_by_dependency_depth() {
    let sys: &'static dyn System = Box::leak(Box::new(tsrs_execute::sys::new_system()));
    let host = tsrs_execute::execute::parse_config_host(sys);
    let command = P::new(tsrs_tsoptions::parse_build_command_line(
        &["--build".into(), "--builders".into(), "1".into()],
        host,
    ));
    let orchestrator = new_orchestrator(Options { sys, command, testing: None });

    let names = ["Leaf", "Middle", "Independent", "Root", "Other"];
    let mut tasks = FxHashMap::default();
    for name in names {
        let config = format!("/project/{name}/tsconfig.json");
        let task = P::new(BuildTask::new(config.clone(), true));
        orchestrator.tasks.lock().unwrap().insert(orchestrator.to_path(&config), task);
        tasks.insert(name, task);
        orchestrator.order.lock().unwrap().push(config);
    }
    tasks["Middle"]
        .up_stream
        .lock()
        .unwrap()
        .push(UpstreamTask { task: tasks["Leaf"], ref_index: 0 });
    tasks["Root"].up_stream.lock().unwrap().extend([
        UpstreamTask { task: tasks["Middle"], ref_index: 0 },
        UpstreamTask { task: tasks["Independent"], ref_index: 1 },
    ]);

    let full = orchestrator.order();
    let selected = full[..4].to_vec();
    let references = full[..3].to_vec();
    let project_names = |order: Vec<String>| -> Vec<String> {
        order.iter().map(|config| config.split('/').nth(2).unwrap().to_string()).collect()
    };
    assert_eq!(
        project_names(orchestrator.compute_schedule_order(&full)),
        ["Leaf", "Independent", "Other", "Middle", "Root"]
    );
    assert_eq!(
        project_names(orchestrator.compute_schedule_order(&selected)),
        ["Leaf", "Independent", "Middle", "Root"]
    );
    let schedule = orchestrator.compute_schedule_order(&references);
    assert_eq!(project_names(schedule.clone()), ["Leaf", "Independent", "Middle"]);

    // A single builder takes exactly the selected projects in their computed order.
    let dispatched = Mutex::new(Vec::new());
    orchestrator.range_tasks(&schedule, &|_, task| {
        dispatched.lock().unwrap().push(task.config.clone());
    });
    assert_eq!(project_names(dispatched.into_inner().unwrap()), ["Leaf", "Independent", "Middle"]);
    assert_eq!(project_names(orchestrator.order()), names);
}
