//! Claude 官方状态页（Statuspage 公共 JSON）。
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    /// none | minor | major | critical | maintenance
    pub indicator: String,
    pub description: String,
}

pub async fn fetch(client: &reqwest::Client, url: &str) -> anyhow::Result<Status> {
    let v: Value = client.get(url).send().await?.error_for_status()?.json().await?;
    let st = v.get("status").cloned().unwrap_or(Value::Null);
    Ok(Status {
        indicator: st.get("indicator").and_then(|x| x.as_str()).unwrap_or("").into(),
        description: st.get("description").and_then(|x| x.as_str()).unwrap_or("").into(),
    })
}
