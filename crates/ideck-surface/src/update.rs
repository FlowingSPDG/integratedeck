use serde::{Deserialize, Serialize};

use crate::CellAddress;
use ideck_core::VisualState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellUpdate {
    pub address: CellAddress,
    pub visual: VisualState,
}
