use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RlpItem {
    Bytes(Bytes),
    List(Vec<RlpItem>),
}

impl RlpItem {
    pub fn bytes(data: impl Into<Bytes>) -> Self {
        RlpItem::Bytes(data.into())
    }

    pub fn list(items: Vec<RlpItem>) -> Self {
        RlpItem::List(items)
    }
}
