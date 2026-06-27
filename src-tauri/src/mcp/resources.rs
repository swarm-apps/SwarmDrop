//! MCP Resource 实现
//!
//! 提供 swarmdrop://guide 使用指南 Resource

use rmcp::ErrorData;
use rmcp::model::{
    AnnotateAble, ListResourcesResult, RawResource, ReadResourceRequestParams, ReadResourceResult,
    ResourceContents,
};

/// 使用指南文档（编译时嵌入）
const GUIDE: &str = include_str!("../../docs/mcp-guide.md");

/// 返回可用 Resource 列表
pub fn list() -> ListResourcesResult {
    let resource = RawResource::new("swarmdrop://guide", "SwarmDrop MCP 使用指南")
        .with_description("SwarmDrop MCP Tool 的使用说明，包含前置条件、Tool 使用顺序和典型流程")
        .with_mime_type("text/markdown")
        .no_annotation();

    ListResourcesResult::with_all_items(vec![resource])
}

/// 读取指定 Resource
pub fn read(request: ReadResourceRequestParams) -> Result<ReadResourceResult, ErrorData> {
    match request.uri.as_str() {
        "swarmdrop://guide" => Ok(ReadResourceResult::new(vec![ResourceContents::text(
            GUIDE,
            "swarmdrop://guide",
        )])),
        _ => Err(ErrorData::resource_not_found(
            format!("Resource not found: {}", request.uri),
            None,
        )),
    }
}
