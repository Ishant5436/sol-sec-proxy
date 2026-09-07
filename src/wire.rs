use std::fmt;

pub const COMPUTE_BUDGET_PROGRAM_ID_STR: &str = "ComputeBudget111111111111111111111111111111";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionVersion {
    Legacy,
    V0,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHeader {
    pub num_required_signatures: u8,
    pub num_readonly_signed_accounts: u8,
    pub num_readonly_unsigned_accounts: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInstruction {
    pub program_id_index: u8,
    pub accounts: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComputeBudgetInstruction {
    RequestHeapFrame(u32),
    SetComputeUnitLimit(u32),
    SetComputeUnitPrice(u64),
    SetLoadedAccountsDataSizeLimit(u32),
    Unknown(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTransaction {
    pub version: TransactionVersion,
    pub signatures: Vec<[u8; 64]>,
    pub header: MessageHeader,
    pub account_keys: Vec<[u8; 32]>,
    pub recent_blockhash: [u8; 32],
    pub instructions: Vec<CompiledInstruction>,
    pub compute_unit_limit: Option<u32>,
    pub compute_unit_price: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    UnexpectedEndOfBuffer,
    InvalidEncoding(String),
    InvalidShortVec,
    InvalidVersion(u8),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEndOfBuffer => write!(f, "Unexpected end of transaction wire buffer"),
            Self::InvalidEncoding(s) => write!(f, "Invalid encoding: {}", s),
            Self::InvalidShortVec => write!(f, "Invalid short-vec length encoding"),
            Self::InvalidVersion(v) => write!(f, "Unsupported transaction version: {}", v),
        }
    }
}

pub struct WireReader<'a> {
    buffer: &'a [u8],
    offset: usize,
}

impl<'a> WireReader<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        assert!(!buffer.is_empty(), "Wire buffer must not be empty");
        Self { buffer, offset: 0 }
    }

    pub fn remaining(&self) -> usize {
        assert!(
            self.offset <= self.buffer.len(),
            "Offset boundary invariant"
        );
        self.buffer.len().saturating_sub(self.offset)
    }

    pub fn read_u8(&mut self) -> Result<u8, WireError> {
        assert!(
            self.offset <= self.buffer.len(),
            "Offset invariant in read_u8"
        );
        if self.offset >= self.buffer.len() {
            return Err(WireError::UnexpectedEndOfBuffer);
        }
        let b = self.buffer[self.offset];
        self.offset += 1;
        Ok(b)
    }

    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], WireError> {
        assert!(
            self.offset <= self.buffer.len(),
            "Offset invariant in read_bytes"
        );
        if self.remaining() < len {
            return Err(WireError::UnexpectedEndOfBuffer);
        }
        let slice = &self.buffer[self.offset..self.offset + len];
        self.offset += len;
        assert_eq!(slice.len(), len, "Slice len matches requested len");
        Ok(slice)
    }

    pub fn read_short_vec_len(&mut self) -> Result<usize, WireError> {
        assert!(
            self.offset <= self.buffer.len(),
            "Offset invariant in short-vec"
        );
        let mut len: usize = 0;
        let mut size: usize = 0;
        let mut loop_count: usize = 0;

        while loop_count < 3 {
            loop_count += 1;
            let byte = self.read_u8()? as usize;
            len |= (byte & 0x7f) << (size * 7);
            size += 1;
            if (byte & 0x80) == 0 {
                assert!(len <= 65535, "Short-vec length bound");
                return Ok(len);
            }
        }
        Err(WireError::InvalidShortVec)
    }
}

pub fn parse_transaction_wire(raw_bytes: &[u8]) -> Result<ParsedTransaction, WireError> {
    assert!(!raw_bytes.is_empty(), "Wire payload must not be empty");
    assert!(
        raw_bytes.len() <= 1280,
        "Solana MTU limit bound (1280 bytes)"
    );

    let mut reader = WireReader::new(raw_bytes);

    let num_signatures = reader.read_short_vec_len()?;
    if num_signatures > 64 {
        return Err(WireError::InvalidEncoding(
            "Excessive signatures count".to_string(),
        ));
    }

    let mut signatures = Vec::with_capacity(num_signatures);
    let mut sig_idx = 0;
    while sig_idx < num_signatures {
        sig_idx += 1;
        let sig_bytes = reader.read_bytes(64)?;
        let mut sig = [0u8; 64];
        sig.copy_from_slice(sig_bytes);
        signatures.push(sig);
    }

    let first_byte = reader.read_u8()?;
    let (version, header) = if (first_byte & 0x80) != 0 {
        let v = first_byte & 0x7f;
        if v != 0 {
            return Err(WireError::InvalidVersion(v));
        }
        let hdr = MessageHeader {
            num_required_signatures: reader.read_u8()?,
            num_readonly_signed_accounts: reader.read_u8()?,
            num_readonly_unsigned_accounts: reader.read_u8()?,
        };
        (TransactionVersion::V0, hdr)
    } else {
        let hdr = MessageHeader {
            num_required_signatures: first_byte,
            num_readonly_signed_accounts: reader.read_u8()?,
            num_readonly_unsigned_accounts: reader.read_u8()?,
        };
        (TransactionVersion::Legacy, hdr)
    };

    let num_accounts = reader.read_short_vec_len()?;
    if num_accounts > 256 {
        return Err(WireError::InvalidEncoding(
            "Excessive accounts count".to_string(),
        ));
    }

    let mut account_keys = Vec::with_capacity(num_accounts);
    let mut acc_idx = 0;
    while acc_idx < num_accounts {
        acc_idx += 1;
        let acc_bytes = reader.read_bytes(32)?;
        let mut key = [0u8; 32];
        key.copy_from_slice(acc_bytes);
        account_keys.push(key);
    }

    let blockhash_bytes = reader.read_bytes(32)?;
    let mut recent_blockhash = [0u8; 32];
    recent_blockhash.copy_from_slice(blockhash_bytes);

    let num_instructions = reader.read_short_vec_len()?;
    if num_instructions > 64 {
        return Err(WireError::InvalidEncoding(
            "Excessive instructions count".to_string(),
        ));
    }

    let mut instructions = Vec::with_capacity(num_instructions);
    let mut cu_limit: Option<u32> = None;
    let mut cu_price: Option<u64> = None;

    let compute_budget_key = bs58::decode(COMPUTE_BUDGET_PROGRAM_ID_STR)
        .into_vec()
        .unwrap_or_default();

    let mut ix_idx = 0;
    while ix_idx < num_instructions {
        ix_idx += 1;
        let program_id_index = reader.read_u8()?;
        let num_ix_accounts = reader.read_short_vec_len()?;
        let accounts_slice = reader.read_bytes(num_ix_accounts)?;
        let data_len = reader.read_short_vec_len()?;
        let data_slice = reader.read_bytes(data_len)?;

        if (program_id_index as usize) < account_keys.len()
            && account_keys[program_id_index as usize] == compute_budget_key.as_slice()
            && !data_slice.is_empty()
        {
            match parse_compute_budget_instruction(data_slice) {
                Some(ComputeBudgetInstruction::SetComputeUnitLimit(limit)) => {
                    cu_limit = Some(limit);
                }
                Some(ComputeBudgetInstruction::SetComputeUnitPrice(price)) => {
                    cu_price = Some(price);
                }
                _ => {}
            }
        }

        instructions.push(CompiledInstruction {
            program_id_index,
            accounts: accounts_slice.to_vec(),
            data: data_slice.to_vec(),
        });
    }

    assert!(
        account_keys.len() >= header.num_required_signatures as usize,
        "Signature count invariant"
    );
    assert!(signatures.len() <= 64, "Signatures upper bound");

    Ok(ParsedTransaction {
        version,
        signatures,
        header,
        account_keys,
        recent_blockhash,
        instructions,
        compute_unit_limit: cu_limit,
        compute_unit_price: cu_price,
    })
}

pub fn parse_compute_budget_instruction(data: &[u8]) -> Option<ComputeBudgetInstruction> {
    assert!(!data.is_empty(), "Compute budget data must not be empty");
    assert!(data.len() <= 64, "Compute budget instruction bound");

    let disc = data[0];
    match disc {
        1 if data.len() >= 5 => {
            let val = u32::from_le_bytes(data[1..5].try_into().ok()?);
            Some(ComputeBudgetInstruction::RequestHeapFrame(val))
        }
        2 if data.len() >= 5 => {
            let val = u32::from_le_bytes(data[1..5].try_into().ok()?);
            Some(ComputeBudgetInstruction::SetComputeUnitLimit(val))
        }
        3 if data.len() >= 9 => {
            let val = u64::from_le_bytes(data[1..9].try_into().ok()?);
            Some(ComputeBudgetInstruction::SetComputeUnitPrice(val))
        }
        4 if data.len() >= 5 => {
            let val = u32::from_le_bytes(data[1..5].try_into().ok()?);
            Some(ComputeBudgetInstruction::SetLoadedAccountsDataSizeLimit(
                val,
            ))
        }
        _ => Some(ComputeBudgetInstruction::Unknown(disc)),
    }
}

pub fn decode_transaction_from_wire_string(encoded: &str) -> Result<ParsedTransaction, WireError> {
    assert!(
        !encoded.is_empty(),
        "Encoded transaction string must not be empty"
    );
    assert!(encoded.len() <= 4096, "Encoded string length bound");

    let bytes = if let Ok(decoded) = bs58::decode(encoded).into_vec() {
        decoded
    } else {
        use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
        use base64::Engine;
        BASE64_STANDARD.decode(encoded).map_err(|e| {
            WireError::InvalidEncoding(format!("Base58/Base64 decode failed: {}", e))
        })?
    };

    parse_transaction_wire(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_vec_reading() {
        let buf = [0x01, 0x80, 0x01, 0xff, 0x01];
        let mut reader = WireReader::new(&buf);
        assert_eq!(reader.read_short_vec_len().unwrap(), 1);
        assert_eq!(reader.read_short_vec_len().unwrap(), 128);
        assert_eq!(reader.read_short_vec_len().unwrap(), 255);
    }

    #[test]
    fn test_compute_budget_parsing() {
        let set_limit = [2u8, 0x40, 0x42, 0x0f, 0x00]; // 1,000,000 CU
        let parsed = parse_compute_budget_instruction(&set_limit).unwrap();
        assert_eq!(
            parsed,
            ComputeBudgetInstruction::SetComputeUnitLimit(1_000_000)
        );

        let set_price = [3u8, 0xe8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // 1,000 micro-lamports
        let parsed_price = parse_compute_budget_instruction(&set_price).unwrap();
        assert_eq!(
            parsed_price,
            ComputeBudgetInstruction::SetComputeUnitPrice(1_000)
        );
    }
}
