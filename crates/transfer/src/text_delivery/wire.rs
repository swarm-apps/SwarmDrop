//! 文本投递专用 RPC wire。

use serde::{Deserialize, Serialize};
use swarmdrop_net::{ProtocolId, Rpc};
use uuid::Uuid;

/// 纯文本投递协议。它与文件控制/数据面分开，避免正文进入分块、路径或会话恢复语义。
pub const TEXT_DELIVERY_PROTOCOL: ProtocolId =
    ProtocolId::from_static("/swarmdrop/text-delivery/1");

pub const TEXT_DELIVERY: Rpc<TextDeliveryRequest, TextDeliveryResponse> =
    Rpc::new(TEXT_DELIVERY_PROTOCOL);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TextDeliveryRequest {
    Deliver { delivery_id: Uuid, body: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TextDeliveryResponse {
    Delivered { inbox_item_id: Uuid },
    Rejected { reason: TextDeliveryRejectReason },
    Expired,
}

/// 只含发送方可安全理解的拒绝类别；不泄露接收端具体策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDeliveryRejectReason {
    NotPaired,
    ReceivingPaused,
    PolicyRejected,
    InvalidPayload,
    QueueFull,
    ProtocolConflict,
    StorageUnavailable,
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{TextDeliveryRejectReason, TextDeliveryRequest, TextDeliveryResponse};

    #[test]
    fn request_wire_keeps_the_existing_delivery_identifier_shape() {
        let delivery_id = Uuid::nil();
        let request = TextDeliveryRequest::Deliver {
            delivery_id,
            body: "文本".as_bytes().to_vec(),
        };

        let encoded = serde_json::to_value(&request).expect("编码文本投递请求");
        assert_eq!(encoded["kind"], "deliver");
        assert_eq!(encoded["delivery_id"], delivery_id.to_string());
        assert_eq!(
            serde_json::from_value::<TextDeliveryRequest>(encoded).expect("解码文本投递请求"),
            request
        );
    }

    #[test]
    fn response_wire_keeps_the_snake_case_rejection_reason() {
        let response = TextDeliveryResponse::Rejected {
            reason: TextDeliveryRejectReason::StorageUnavailable,
        };

        let encoded = serde_json::to_value(&response).expect("编码文本投递响应");
        assert_eq!(encoded["kind"], "rejected");
        assert_eq!(encoded["reason"], "storage_unavailable");
        assert_eq!(
            serde_json::from_value::<TextDeliveryResponse>(encoded).expect("解码文本投递响应"),
            response
        );
    }
}
