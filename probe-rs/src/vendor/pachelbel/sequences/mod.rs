//! Pachelbel debug sequences.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use probe_rs_target::CoreType;

use crate::{
    MemoryMappedRegister,
    architecture::arm::{
        ArmError,
        armv8m::{Demcr, Dhcsr},
        memory::ArmMemoryInterface,
        sequences::ArmDebugSequence,
    },
};

/// Marker struct indicating initialization sequencing for Pachelbel Doksan NPU family parts.
#[derive(Debug)]
pub struct DoksanNpu {}

impl DoksanNpu {
    /// Create the sequencer for the Doksan NPU family of parts.
    pub fn create() -> Arc<Self> {
        Arc::new(Self {})
    }

    /// Release the CPU core from Reset Extension
    pub fn release_reset_extension(
        &self,
        memory: &mut dyn ArmMemoryInterface,
    ) -> Result<(), ArmError> {
        // Halt the core
        let mut dhcsr = Dhcsr(0);
        dhcsr.enable_write();
        dhcsr.set_c_halt(true);
        dhcsr.set_c_debugen(true);
        memory.write_word_32(Dhcsr::get_mmio_address(), dhcsr.into())?;
        memory.flush()?;

        // Clear VECTOR CATCH and set TRCENA
        let mut demcr: Demcr = memory.read_word_32(Demcr::get_mmio_address())?.into();
        demcr.set_trcena(true);
        memory.write_word_32(Demcr::get_mmio_address(), demcr.into())?;
        memory.flush()?;

        Ok(())
    }

    /// Halt or unhalt the core.
    fn halt(&self, memory: &mut dyn ArmMemoryInterface, halt: bool) -> Result<(), ArmError> {
        let mut dhcsr = Dhcsr(memory.read_word_32(Dhcsr::get_mmio_address())?);
        dhcsr.set_c_halt(halt);
        dhcsr.set_c_debugen(true);
        dhcsr.enable_write();
        memory.write_word_32(Dhcsr::get_mmio_address(), dhcsr.into())?;
        memory.flush()?;

        let start = Instant::now();
        let action = if halt { "halt" } else { "unhalt" };

        while Dhcsr(memory.read_word_32(Dhcsr::get_mmio_address())?).s_halt() != halt {
            if start.elapsed() > Duration::from_millis(100) {
                tracing::debug!("Exceeded timeout while waiting for the core to {action}");
                return Err(ArmError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        Ok(())
    }

    /// Poll the AP's status until it can accept transfers.
    fn wait_for_enable(
        &self,
        memory: &mut dyn ArmMemoryInterface,
        timeout: Duration,
    ) -> Result<(), ArmError> {
        let start = Instant::now();
        let mut errors = 0usize;
        let mut disables = 0usize;

        loop {
            match memory.generic_status() {
                Ok(csw) if csw.DeviceEn => {
                    tracing::debug!(
                        "Device enabled after {}ms with {errors} errors and {disables} invalid statuses",
                        start.elapsed().as_millis()
                    );
                    return Ok(());
                }
                Ok(_) => disables += 1,
                Err(_) => errors += 1,
            }

            if start.elapsed() > timeout {
                tracing::debug!(
                    "Exceeded {}ms timeout while waiting for enable with {errors} errors and {disables} invalid statuses",
                    timeout.as_millis()
                );
                return Err(ArmError::Timeout);
            }

            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl ArmDebugSequence for DoksanNpu {
    fn reset_system(
        &self,
        memory: &mut dyn ArmMemoryInterface,
        core_type: CoreType,
        debug_base: Option<u64>,
    ) -> Result<(), ArmError> {
        use crate::architecture::arm::core::armv8m::Aircr;

        let _init_nsvtor = memory.read_word_32(0x4000E004)?;
        // let vec_sp = memory.read_word_32(init_nsvtor as u64)?;
        // let vec_reset = memory.read_word_32(init_nsvtor as u64 + 4)?;
        // if vec_sp == 0xffffffff || vec_reset == 0xffffffff {
        //     tracing::warn!("Use temporary vector to avoid LockUp");
        //     //
        // } else {
        //     tracing::info!("sp {vec_sp:08x}, reset {vec_reset:08x}");
        // }
        memory.write_word_32(0x00000000, 0x00040000)?;
        memory.write_word_32(0x00000004, 0x0003FFF1)?;
        memory.write_word_32(0x0003FFF0, 0xbf00e7fe)?;
        memory.write_word_32(0x4000E004, 0x00000001)?;

        let mut aircr = Aircr(0);
        aircr.vectkey();
        aircr.set_sysresetreq(true);
        memory
            .write_word_32(Aircr::get_mmio_address(), aircr.into())
            .ok();
        memory.flush().ok();

        // If all goes well, we lost the debug port. Thanks, boot ROM. Let's bring it back.
        //
        // The ARM communication interface knows how to re-initialize the debug port.
        // Re-initializing the core(s) is on us.
        let ap = memory.fully_qualified_address();
        let interface = memory.get_arm_debug_interface()?;
        interface.reinitialize()?;

        assert!(debug_base.is_none());
        self.debug_core_start(interface, &ap, core_type, None, None)?;

        // Are we back?
        self.wait_for_enable(memory, Duration::from_millis(300))?;

        // TODO: reset_catch_set

        // We're back. Halt the core so we can establish the reset context.
        let mut dhcsr = Dhcsr(memory.read_word_32(Dhcsr::get_mmio_address())?);
        dhcsr.set_c_halt(true);
        dhcsr.set_c_debugen(true);
        dhcsr.enable_write();
        memory.write_word_32(Dhcsr::get_mmio_address(), dhcsr.into())?;

        memory.write_word_32(0x4000E004, 0x00000000)?;
        memory.flush()?;

        let start = Instant::now();

        while !Dhcsr(memory.read_word_32(Dhcsr::get_mmio_address())?).s_halt() {
            if start.elapsed() > Duration::from_millis(100) {
                tracing::debug!("Exceeded timeout while waiting for the core to halt");
                return Err(ArmError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        Ok(())
    }
}
