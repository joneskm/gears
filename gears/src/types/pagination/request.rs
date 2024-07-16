use serde::Deserialize;

use crate::ext::{Pagination, PaginationByKey, PaginationByOffset};

pub(crate) const QUERY_DEFAULT_LIMIT: u8 = 100;

//#[derive(FromForm, Debug)]
#[derive(Deserialize, serde::Serialize, Debug, Clone, Eq, PartialEq)]
pub struct PaginationRequest {
    /// key is a value returned in PageResponse.next_key to begin
    /// querying the next page most efficiently. Only one of offset or key
    /// should be set.
    pub key: Option<Vec<u8>>,
    /// offset is a numeric offset that can be used when key is unavailable.
    /// It is less efficient than using key. Only one of offset or key should
    /// be set.
    pub offset: u32,
    /// limit is the total number of results to be returned in the result page.
    /// If left empty it will default to a value to be set by each app.
    pub limit: u8,
}

impl From<PaginationRequest> for Pagination {
    fn from(PaginationRequest { offset, limit, key }: PaginationRequest) -> Self {
        match key {
            Some(key) => Self::from(PaginationByKey {
                key,
                limit: limit as usize,
            }),
            None => Self::from(PaginationByOffset {
                offset: offset
                    .checked_mul(limit as u32)
                    .map(|this| this as usize)
                    .unwrap_or(usize::MAX),
                limit: limit as usize,
            }),
        }
    }
}

impl Default for PaginationRequest {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: QUERY_DEFAULT_LIMIT,
            key: None,
        }
    }
}

impl From<core_types::query::request::PageRequest> for PaginationRequest {
    fn from(
        core_types::query::request::PageRequest {
            key,
            offset,
            limit,
            count_total: _,
            reverse: _,
        }: core_types::query::request::PageRequest,
    ) -> Self {
        Self {
            offset: offset.try_into().unwrap_or(u32::MAX),
            limit: limit.try_into().unwrap_or(u8::MAX),
            key: match key.is_empty() {
                true => None,
                false => Some(key),
            },
        }
    }
}

impl From<PaginationRequest> for core_types::query::request::PageRequest {
    fn from(PaginationRequest { offset, limit, key }: PaginationRequest) -> Self {
        Self {
            key: key.unwrap_or_default(),
            offset: offset as u64,
            limit: limit as u64,
            count_total: false,
            reverse: false,
        }
    }
}