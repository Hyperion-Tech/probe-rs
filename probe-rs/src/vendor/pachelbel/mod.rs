//! Pachelbel vendor support.

use probe_rs_target::Chip;

use crate::{
    config::DebugSequence,
    vendor::{Vendor, pachelbel::sequences::DoksanNpu},
};

pub mod sequences;

/// Pachelbel
#[derive(docsplay::Display)]
pub struct Pachelbel;

impl Vendor for Pachelbel {
    fn try_create_debug_sequence(&self, chip: &Chip) -> Option<DebugSequence> {
        let sequence = if chip.name.starts_with("Doksan") {
            DebugSequence::Arm(DoksanNpu::create())
        } else {
            return None;
        };

        Some(sequence)
    }
}
