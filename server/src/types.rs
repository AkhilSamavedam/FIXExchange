use fefix::{Buffer, FixValue};

pub(crate) type OrderID = u64;

#[derive(Debug, Clone)]
pub(crate) struct ClientID {
    comp_id: String,
    sub_id: Option<String>
}

impl ClientID {
    pub(crate) fn new(comp_id: String, sub_id: Option<String>) -> Self {
        Self { comp_id, sub_id }
    }
}

impl<'a> FixValue<'a> for ClientID {
    type Error = ();
    type SerializeSettings = ();

    fn serialize_with<B>(&self, buffer: &mut B, _settings: Self::SerializeSettings) -> usize
    where
        B: Buffer,
    {
        let s = match &self.sub_id {
            Some(sub) => format!("{}|{}", self.comp_id, sub),
            None => self.comp_id.clone(),
        };
        buffer.extend_from_slice(s.as_bytes());
        s.len()
    }

    fn deserialize(data: &'a [u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(data).map_err(|_| ())?;
        let mut parts = s.splitn(2, '|');
        let comp_id = parts.next().unwrap_or("").to_string();
        let sub_id = parts.next().map(|s| s.to_string());
        Ok(ClientID { comp_id, sub_id })
    }
}

pub(crate) type InstrumentID = String;
pub(crate) type Quantity = u64;
pub(crate) type Price = f64;
pub(crate) type AccountBalance = i64;

pub(crate) type AccountID = String;
