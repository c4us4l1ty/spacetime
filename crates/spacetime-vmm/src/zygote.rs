//! The zygote: one paused golden VM whose memory every new tab clones.
//!
//! Cold-booting a stripped kernel costs ~50–100 ms. Spacetime boots a single
//! golden guest at startup, pauses it once `init` is ready, and snapshots its
//! memory. New tabs then clone that snapshot, so `+ New Tab` costs the clone
//! only — single-digit milliseconds.
//!
//! In the real backend a clone is a `memfd` + `mmap(MAP_PRIVATE)` of the golden
//! pages handed to the new guest RAM slot; both VMs map the *same* physical
//! host pages until either one writes. The simulation models the timing and
//! bookkeeping so policies (idle reaping, warm-up, capacity) can be tested now.

use std::time::{Duration, Instant};

use spacetime_core::MemoryImage;

use crate::{VmmError, VmConfig};

/// Lifecycle of the golden VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZygoteState {
    /// Not started.
    Cold,
    /// Kernel booting; not cloneable yet.
    Booting,
    /// Paused and snapshotted; can serve clones.
    Ready,
    /// Reaped; must be rebuilt before the next clone.
    Retired,
}

/// A paused golden microVM.
#[derive(Debug)]
pub struct Zygote {
    state: ZygoteState,
    image: MemoryImage,
    boot_ms: u32,
    /// Page-cache footprint of the golden image, charged once to the host.
    pub shared_mb: u64,
    clones_served: u64,
    /// Number of clones after which the golden VM is recycled. Dirty kernel
    /// state (entropy pool, timers) accumulates in the CoW layer over time, so
    /// real backends refresh the zygote periodically.
    recycle_after: u64,
}

impl Default for Zygote {
    fn default() -> Self {
        Zygote::new()
    }
}

impl Zygote {
    pub fn new() -> Zygote {
        Zygote {
            state: ZygoteState::Cold,
            image: MemoryImage::new(),
            boot_ms: 0,
            shared_mb: 48,
            clones_served: 0,
            recycle_after: 512,
        }
    }

    /// Boot the golden VM. In the simulation the "boot" populates the base
    /// memory image exactly as a real guest's kernel + init would.
    pub fn boot(&mut self) -> Result<u32, VmmError> {
        if self.state == ZygoteState::Booting {
            return Err(VmmError::BadState {
                tab: spacetime_core::TabId(0),
                wanted: "not already booting",
            });
        }
        self.state = ZygoteState::Booting;
        let started = Instant::now();
        std::thread::sleep(Duration::from_millis(3));
        let mut memory = MemoryImage::new();
        memory.fill(0, 96, 0x5a);
        self.image = memory;
        self.boot_ms = started.elapsed().as_millis().max(1) as u32;
        self.state = ZygoteState::Ready;
        Ok(self.boot_ms)
    }

    /// Hand out one clone. Returns the clone's boot-to-ready time in ms.
    pub fn take_clone(&mut self) -> Result<u32, VmmError> {
        match self.state {
            ZygoteState::Ready => {
                self.clones_served += 1;
                // Clone cost grows slowly with served clones (dirty CoW pages).
                let ms = 5 + ((self.clones_served * 7) % 11) as u32;
                if self.clones_served >= self.recycle_after {
                    self.state = ZygoteState::Retired;
                }
                Ok(ms)
            }
            ZygoteState::Cold => Err(VmmError::Unavailable("zygote not booted")),
            ZygoteState::Booting => Err(VmmError::BadState {
                tab: spacetime_core::TabId(0),
                wanted: "not booting",
            }),
            ZygoteState::Retired => Err(VmmError::Unavailable("zygote retired; reboot it")),
        }
    }

    pub fn state(&self) -> ZygoteState { self.state }

    pub fn image(&self) -> &MemoryImage { &self.image }

    pub fn clones_served(&self) -> u64 { self.clones_served }

    pub fn boot_ms(&self) -> u32 { self.boot_ms }

    /// Host RAM the golden image costs once, no matter how many tabs exist.
    pub fn shared_ram_mb(&self) -> u64 { self.shared_mb }

    /// Configure for a specific VM config (image path, shared mounts).
    pub fn prepare(&mut self, config: &VmConfig) -> Result<(), VmmError> {
        if config.image.is_empty() {
            return Err(VmmError::Backend("empty image path".into()));
        }
        Ok(())
    }
}

impl ZygoteState {
    pub fn label(self) -> &'static str {
        match self {
            ZygoteState::Cold => "cold",
            ZygoteState::Booting => "booting",
            ZygoteState::Ready => "ready",
            ZygoteState::Retired => "retired",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_zygote_refuses_clones() {
        let mut zygote = Zygote::new();
        assert!(zygote.take_clone().is_err());
    }

    #[test]
    fn boot_then_clone_is_fast() {
        let mut zygote = Zygote::new();
        let cold_boot = zygote.boot().unwrap();
        assert!(cold_boot >= 3);
        assert_eq!(zygote.state(), ZygoteState::Ready);
        let ms = zygote.take_clone().unwrap();
        assert!(ms < cold_boot.max(10));
        assert!(ms <= 15);
        assert_eq!(zygote.clones_served(), 1);
    }

    #[test]
    fn zygote_recycles_after_capacity() {
        let mut zygote = Zygote::new();
        zygote.boot().unwrap();
        zygote.recycle_after = 3;
        for _ in 0..3 {
            zygote.take_clone().unwrap();
        }
        assert_eq!(zygote.state(), ZygoteState::Retired);
        assert!(zygote.take_clone().is_err());
        zygote.boot().unwrap();
        assert_eq!(zygote.state(), ZygoteState::Ready);
    }

    #[test]
    fn shared_ram_is_charged_once() {
        let zygote = Zygote::new();
        assert_eq!(zygote.shared_ram_mb(), 48);
    }
}
