//! 바둑·오목 규칙 엔진 + 자체 AI (바둑: MCTS+RAVE, 오목: 알파-베타 탐색).
//!
//! C#에서 호출하는 C ABI는 두 함수뿐이다.
//! - `engine_call(json)`: `{"cmd": "...", ...}` 요청을 받아 JSON 응답 문자열을 돌려준다.
//! - `engine_free(ptr)`: `engine_call`이 돌려준 문자열을 해제한다.
//!
//! 엔진은 상태를 갖지 않는다. 게임 상태는 JSON으로 C# 쪽에 보관되고 매 요청마다 넘어온다.

pub mod coord;
pub mod go;
pub mod go_ai;
pub mod omok;
pub mod omok_ai;

use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};
use std::panic;

fn field<T: serde::de::DeserializeOwned>(req: &Value, name: &str) -> Result<T, String> {
    let v = req.get(name).ok_or_else(|| format!("'{name}' 필드가 없습니다"))?;
    serde_json::from_value(v.clone()).map_err(|e| format!("'{name}' 형식 오류: {e}"))
}

fn opt_field<T: serde::de::DeserializeOwned>(req: &Value, name: &str, default: T) -> Result<T, String> {
    match req.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(_) => field(req, name),
    }
}

/// 요청 하나를 처리한다. (FFI 없이 Rust 테스트에서도 사용)
pub fn dispatch(req: &Value) -> Result<Value, String> {
    let cmd: String = field(req, "cmd")?;
    match cmd.as_str() {
        // ---------------- 바둑
        "go_new" => {
            let size: usize = opt_field(req, "size", 19)?;
            let komi: f32 = opt_field(req, "komi", 6.5)?;
            Ok(json!({ "state": go::GoState::new(size, komi)? }))
        }
        "go_play" => {
            let mut st: go::GoState = field(req, "state")?;
            let mv: String = field(req, "move")?;
            let captured = st.play_coord(&mv)?;
            Ok(json!({ "state": st, "captured": captured }))
        }
        "go_info" => {
            let st: go::GoState = field(req, "state")?;
            Ok(json!({ "ascii": st.ascii(), "illegal": st.illegal_points() }))
        }
        "go_genmove" => {
            let st: go::GoState = field(req, "state")?;
            let ms: u64 = opt_field(req, "ms", 3000)?;
            Ok(serde_json::to_value(go_ai::genmove(&st, ms)).unwrap())
        }
        "go_dead" => {
            let st: go::GoState = field(req, "state")?;
            let ms: u64 = opt_field(req, "ms", 1500)?;
            Ok(json!({ "dead": go_ai::dead_stones(&st, ms) }))
        }
        "go_score" => {
            let st: go::GoState = field(req, "state")?;
            let dead: Vec<String> = opt_field(req, "dead", vec![])?;
            Ok(serde_json::to_value(st.score(&dead)).unwrap())
        }
        // ---------------- 오목
        "omok_new" => Ok(json!({ "state": omok::OmokState::new() })),
        "omok_play" => {
            let mut st: omok::OmokState = field(req, "state")?;
            let mv: String = field(req, "move")?;
            st.play_coord(&mv)?;
            Ok(json!({ "state": st }))
        }
        "omok_info" => {
            let st: omok::OmokState = field(req, "state")?;
            Ok(json!({ "ascii": st.ascii(), "threats": st.urgent_points() }))
        }
        "omok_genmove" => {
            let st: omok::OmokState = field(req, "state")?;
            let ms: u64 = opt_field(req, "ms", 1000)?;
            Ok(serde_json::to_value(omok_ai::genmove(&st, ms)).unwrap())
        }
        "engine_info" => Ok(json!({ "threads": go_ai::thread_count() })),
        other => Err(format!("알 수 없는 명령: {other}")),
    }
}

fn handle(raw: &str) -> Value {
    let result = serde_json::from_str::<Value>(raw)
        .map_err(|e| format!("JSON 파싱 오류: {e}"))
        .and_then(|req| dispatch(&req));
    match result {
        Ok(mut v) => {
            v["ok"] = Value::Bool(true);
            v
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

/// # Safety
/// `req`는 NUL로 끝나는 UTF-8 문자열이어야 한다. 반환값은 반드시 `engine_free`로 해제한다.
#[no_mangle]
pub unsafe extern "C" fn engine_call(req: *const c_char) -> *mut c_char {
    let out = panic::catch_unwind(|| {
        if req.is_null() {
            return json!({ "ok": false, "error": "null 요청" });
        }
        match CStr::from_ptr(req).to_str() {
            Ok(s) => handle(s),
            Err(_) => json!({ "ok": false, "error": "UTF-8 아님" }),
        }
    })
    .unwrap_or_else(|_| json!({ "ok": false, "error": "엔진 내부 오류(panic)" }));
    CString::new(out.to_string()).unwrap_or_default().into_raw()
}

/// # Safety
/// `engine_call`이 반환한 포인터만 넘겨야 하며, 두 번 해제하면 안 된다.
#[no_mangle]
pub unsafe extern "C" fn engine_free(p: *mut c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p));
    }
}
