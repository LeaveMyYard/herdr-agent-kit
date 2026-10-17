//! [`Tx`] records of background work: Bash in the background, Monitor
//! watches, their stops and expiries.

use super::*;

impl Tx {
    pub(crate) fn bash_background(&mut self, t: f64, task: &str) -> String {
        let id = self.tid();
        let rec = tpl("assistant_bash_background");
        let input =
            json!({"command": "sleep 60", "description": "watch", "run_in_background": true});
        self.add(tool_block(rec, "Bash", &id, input), t);
        let mut res = self.kind("result_bash_background", "sub_result_bash_background");
        res["toolUseResult"]["backgroundTaskId"] = json!(task);
        self.add(
            result_block(
                res,
                &id,
                &format!("Command running in background with ID: {task}."),
                None,
            ),
            t + 1.0,
        );
        id
    }

    pub(crate) fn monitor(&mut self, t: f64, task: &str, timeout_ms: u64, persistent: bool) {
        let id = self.tid();
        let input = json!({"description": "watch", "timeout_ms": timeout_ms, "command": "true"});
        self.add(
            tool_block(tpl("assistant_monitor"), "Monitor", &id, input),
            t,
        );
        let mut res = tpl("result_monitor");
        tur(
            &mut res,
            json!({"taskId": task, "timeoutMs": timeout_ms, "persistent": persistent}),
        );
        self.add(
            result_block(res, &id, &format!("Monitor started (task {task})."), None),
            t + 1.0,
        );
    }

    pub(crate) fn task_stop(&mut self, t: f64, task: &str, error: bool) {
        let id = self.tid();
        self.add(
            tool_block(
                tpl("assistant_taskstop"),
                "TaskStop",
                &id,
                json!({"task_id": task}),
            ),
            t,
        );
        let mut res = tpl("result_taskstop");
        let msg = format!("Successfully stopped task: {task}");
        tur(
            &mut res,
            json!({"message": msg, "task_id": task, "task_type": "local_bash"}),
        );
        self.add(result_block(res, &id, &msg, Some(error)), t + 1.0);
    }

    pub(crate) fn monitor_expired(&mut self, t: f64, task: &str) {
        let mut q = tpl("queue_enqueue_monitor_expired");
        q["content"] = json!(format!(
            "<task-notification>\n<task-id>{task}</task-id>\n<summary>Monitor event</summary>\n\
             <event>[Monitor expired after 30m]</event>\n</task-notification>"
        ));
        self.add(q, t);
    }
}
