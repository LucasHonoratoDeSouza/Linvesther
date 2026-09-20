//! Multi-account consolidation: aggregate NAV across a track's
//! member set, account addition/removal as non-profit-creating external
//! flows, and unambiguous internal-transfer linkage that nets to zero at
//! the consolidated level with the fee showing up only as a natural NAV
//! reduction.

pub mod aggregate;
pub mod transfer;

pub use aggregate::{
    account_added_flow, account_removed_flow, consolidated_nav, ConsolidationError, MemberAccount,
};
pub use transfer::{link_internal_transfer, InternalTransfer, TransferLeg};
