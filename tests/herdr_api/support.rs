//! Shared helpers: the recorded exchanges, the parity check, short
//! constructors and a scratch directory.

use std::fmt::Debug;
use std::path::PathBuf;

use herdr_agent_kit::herdr_api::fake::{self, FakeHerdr};
use herdr_agent_kit::herdr_api::{Call, HerdrError, PaneId, Sanitized, TabId};
use serde_json::Value;

/// The recorded `(request params, reply frame)` of `method` in one record.
pub fn exchange(file: &str, seq: u64, method: &str) -> (Value, Value) {
    let record = fake::recording(file, seq).unwrap_or_else(|| panic!("{file}:{seq} recorded"));
    fake::exchanges(&record)
        .into_iter()
        .find(|(req, _)| req["method"] == method)
        .map(|(req, reply)| (req.get("params").cloned().unwrap_or(Value::Null), reply))
        .unwrap_or_else(|| panic!("{file}:{seq} has no {method}"))
}

pub fn recorded_param(file: &str, seq: u64, method: &str, pointer: &str) -> String {
    exchange(file, seq, method)
        .0
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

/// What a reply frame means to a call.
pub fn reply_of(frame: &Value) -> Result<Value, HerdrError> {
    match frame.get("error") {
        Some(e) => Err(HerdrError::Refused {
            code: e["code"].as_str().unwrap().to_owned(),
            message: e["message"].as_str().unwrap().to_owned(),
        }),
        None => Ok(frame["result"].clone()),
    }
}

/// The call sends the recorded request, and both clients read what the
/// recorded reply means. Returns that result.
pub async fn parity<T: PartialEq + Debug>(
    call: Call<T>,
    file: &str,
    seq: u64,
) -> Result<T, HerdrError> {
    let (params, frame) = exchange(file, seq, call.method());
    assert_eq!(
        call.params(),
        &params,
        "{file}:{seq} {} params",
        call.method()
    );
    let expected = call.parse(reply_of(&frame));
    let fake = FakeHerdr::recorded();
    fake.replay(file, seq).replay(file, seq); // once per client
    let got = fake.client().send(call.clone()).await;
    assert_eq!(got, expected, "{file}:{seq} async");
    let got = fake.blocking_client().send(call.clone());
    assert_eq!(got, expected, "{file}:{seq} blocking");
    let sent = fake.calls();
    assert_eq!(sent.len(), 2, "every request is recorded");
    for (method, p) in sent {
        assert_eq!((method.as_str(), &p), (call.method(), &params));
    }
    expected
}

pub fn p(id: &str) -> PaneId {
    PaneId::new(id)
}

pub fn t(id: &str) -> TabId {
    TabId::new(id)
}

pub fn text(s: &str) -> Sanitized {
    Sanitized::new(s)
}

/// A private directory of this test's own (removed by the test).
pub fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let dir = std::env::temp_dir().join(format!("{tag}{}-{nanos:x}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
