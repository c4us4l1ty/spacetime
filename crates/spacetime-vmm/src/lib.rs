//! Hardware virtualization abstraction layer.
//!
//! `spacetime-vmm` does not contain a VMM. It defines the [`SpacetimeVmm`] trait
//! that real backends implement, plus a deterministic [`SimulatedBackend`] that
//! lets the shell, agent, network switch and CI all run end-to-end on machines
//! without KVM/HAXM/WHPX (including GitHub runners, which are themselves VMs).
//!
//! Real backends (planned, see PLAN.md):
//!
//! | Backend | Hosts | Engine |
//! |---------|-------|--------|
//! | `KrunBackend` | Linux (KVM), macOS (Hypervisor.framework) | `libkrun` |
//! | `CloudHypervisorBackend` | Linux (KVM/MSHV) | `cloud-hypervisor` |
//! | `CrosvmBackend` | Windows (WHPX), Linux | `crosvm` |
//!
//! All of them are expected to satisfy the same behavioural contract, which the
//! simulation implements faithfully: boot in single-digit milliseconds from a
//! zygote clone, expose a sparse dirty-page memory image, and support pause,
//! snapshot and restore.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use spacetime_core::{Hash, MemoryImage, NetMode, TabId};

pub mod dir_store;
pub mod zygote;

pub use dir_store::DirBackend;
pub use zygote::Zygote;

/// Errors a backend can raise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmmError {
    /// This host cannot run the backend (no KVM, no driver, wrong OS).
    Unavailable(&'static str),
    /// Unknown tab handle.
    UnknownTab(TabId),
    /// The VM exists but is in the wrong lifecycle state.
    BadState { tab: TabId, wanted: &'static str },
    /// Backend-specific failure.
    Backend(String),
}

impl std::fmt::Display for VmmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmmError::Unavailable(why) => write!(f, "backend unavailable: {why}"),
            VmmError::UnknownTab(id) => write!(f, "no VM for tab {id}"),
            VmmError::BadState { tab, wanted } => {
                write!(f, "tab {tab} is not {wanted}")
            }
            VmmError::Backend(msg) => write!(f, "backend error: {msg}"),
        }
    }
}

impl std::error::Error for VmmError {}

/// Everything a backend needs to launch one microVM.
#[derive(Debug, Clone)]
pub struct VmConfig {
    pub tab: TabId,
    pub name: String,
    /// Immutable `.wavefunction` image (rootfs) to boot.
    pub image: String,
    pub vcpus: u8,
    pub ram_mb: u64,
    pub net: NetMode,
    /// Host paths mapped read-only through virtio-fs (the global store).
    pub shared_mounts: Vec<String>,
}

impl VmConfig {
    pub fn new(tab: TabId, name: &str, image: &str, net: NetMode) -> VmConfig {
        VmConfig {
            tab,
            name: name.to_string(),
            image: image.to_string(),
            vcpus: 2,
            ram_mb: 1024,
            net,
            shared_mounts: vec!["/spacetime/store".to_string()],
        }
    }
}

/// Opaque handle to a running VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmHandle {
    pub tab: TabId,
    /// Host thread-group id (real backends) or simulation slot.
    pub pid: u32,
    /// Measured boot-to-ready time.
    pub boot_ms: u32,
}

/// Live counters sampled from a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VmStats {
    pub resident_mb: u64,
    /// Pages the guest handed back through virtio-balloon free-page-reporting.
    pub balloon_reclaimed_mb: u64,
    /// Dirty pages since the last snapshot.
    pub dirty_pages: u64,
    pub vcpu_permille: u32,
}

/// Lifecycle operations every backend must provide.
pub trait SpacetimeVmm {
    /// Short backend name for logs and the shell's status bar.
    fn backend_name(&self) -> &'static str;

    /// Whether this host can actually run the backend.
    fn availability(&self) -> Result<(), VmmError>;

    /// Boot a VM from an immutable image (cold boot, no zygote).
    fn boot(&mut self, config: &VmConfig) -> Result<VmHandle, VmmError>;

    /// Clone a paused zygote's memory with copy-on-write.
    fn clone_from_zygote(
        &mut self,
        zygote: &mut Zygote,
        config: &VmConfig,
    ) -> Result<VmHandle, VmmError>;

    fn pause(&mut self, handle: VmHandle) -> Result<(), VmmError>;
    fn resume(&mut self, handle: VmHandle) -> Result<(), VmmError>;
    fn stop(&mut self, handle: VmHandle) -> Result<(), VmmError>;

    /// Capture the VM's current memory (the input to [`spacetime_core::TimeTravel`]).
    fn snapshot(&mut self, handle: VmHandle) -> Result<MemoryImage, VmmError>;

    /// Restore previously captured memory into a running VM.
    fn restore(&mut self, handle: VmHandle, image: &MemoryImage) -> Result<(), VmmError>;

    fn stats(&self, handle: VmHandle) -> Result<VmStats, VmmError>;

    fn list(&self) -> Vec<VmHandle>;
}

/// Deterministic stand-in for a real hypervisor.
///
/// Semantics copied from the real ones on purpose: boot cost is measured with a
/// real clock, memory is a sparse 4 KiB page map, and `tick()` simulates the
/// guest touching pages (a running app mutates a handful of pages per tick and
/// a "rendering" app mutates more, which is what the scrubber then diff-captures).
#[derive(Debug, Default)]
pub struct SimulatedBackend {
    vms: BTreeMap<TabId, SimVm>,
    next_pid: u32,
}

#[derive(Debug)]
struct SimVm {
    handle: VmHandle,
    config: VmConfig,
    paused: bool,
    stats: VmStats,
    memory: MemoryImage,
    tick: u64,
    balloon_mb: u64,
}

impl SimulatedBackend {
    pub fn new() -> SimulatedBackend {
        SimulatedBackend {
            vms: BTreeMap::new(),
            next_pid: 4001,
        }
    }

    /// Advance every running VM by one "frame": the guest dirties pages, grows
    /// RAM, and occasionally returns free pages via the balloon.
    pub fn tick(&mut self) {
        for vm in self.vms.values_mut() {
            if vm.paused {
                continue;
            }
            vm.tick += 1;
            let dirty = 3 + (vm.tick % 5);
            for offset in 0..dirty {
                let page = (vm.tick * 17 + offset * 4099) % 65_536;
                let byte = ((vm.tick * 31 + offset) % 251) as u8;
                vm.memory.set_page(page, vec![byte; 4096]);
            }
            vm.stats.dirty_pages = dirty;
            vm.stats.resident_mb = vm.config.ram_mb.min(256 + vm.tick * 3);
            if vm.tick % 11 == 0 {
                vm.balloon_mb += 8;
                vm.stats.balloon_reclaimed_mb = vm.balloon_mb;
            }
        }
    }

    pub fn memory_of(&self, tab: TabId) -> Option<&MemoryImage> {
        self.vms.get(&tab).map(|vm| &vm.memory)
    }
}

impl SpacetimeVmm for SimulatedBackend {
    fn backend_name(&self) -> &'static str {
        "simulated"
    }

    fn availability(&self) -> Result<(), VmmError> {
        Ok(())
    }

    fn boot(&mut self, config: &VmConfig) -> Result<VmHandle, VmmError> {
        let started = Instant::now();
        // Cold boot of the stripped kernel takes tens of milliseconds; the last
        // piece of state assembly is modelled here so timing is realistic.
        std::thread::sleep(Duration::from_millis(2));
        let boot_ms = started.elapsed().as_millis().max(1) as u32;
        self.insert(config, boot_ms)
    }

    fn clone_from_zygote(
        &mut self,
        zygote: &mut Zygote,
        config: &VmConfig,
    ) -> Result<VmHandle, VmmError> {
        let boot_ms = zygote.take_clone()?;
        self.insert(config, boot_ms)
    }

    fn pause(&mut self, handle: VmHandle) -> Result<(), VmmError> {
        let vm = self.vms.get_mut(&handle.tab).ok_or(VmmError::UnknownTab(handle.tab))?;
        vm.paused = true;
        Ok(())
    }

    fn resume(&mut self, handle: VmHandle) -> Result<(), VmmError> {
        let vm = self.vms.get_mut(&handle.tab).ok_or(VmmError::UnknownTab(handle.tab))?;
        vm.paused = false;
        Ok(())
    }

    fn stop(&mut self, handle: VmHandle) -> Result<(), VmmError> {
        match self.vms.remove(&handle.tab) {
            // Dropping the entry frees the page map. Real backends additionally
            // zero guest RAM before returning it to the host allocator.
            Some(_) => Ok(()),
            None => Err(VmmError::UnknownTab(handle.tab)),
        }
    }

    fn snapshot(&mut self, handle: VmHandle) -> Result<MemoryImage, VmmError> {
        let vm = self.vms.get(&handle.tab).ok_or(VmmError::UnknownTab(handle.tab))?;
        Ok(vm.memory.clone())
    }

    fn restore(&mut self, handle: VmHandle, image: &MemoryImage) -> Result<(), VmmError> {
        let vm = self.vms.get_mut(&handle.tab).ok_or(VmmError::UnknownTab(handle.tab))?;
        vm.memory = image.clone();
        Ok(())
    }

    fn stats(&self, handle: VmHandle) -> Result<VmStats, VmmError> {
        let vm = self.vms.get(&handle.tab).ok_or(VmmError::UnknownTab(handle.tab))?;
        Ok(vm.stats)
    }

    fn list(&self) -> Vec<VmHandle> {
        self.vms.values().map(|vm| vm.handle).collect()
    }
}

impl SimulatedBackend {
    fn insert(&mut self, config: &VmConfig, boot_ms: u32) -> Result<VmHandle, VmmError> {
        if self.vms.contains_key(&config.tab) {
            return Err(VmmError::BadState {
                tab: config.tab,
                wanted: "not already booted",
            });
        }
        let handle = VmHandle {
            tab: config.tab,
            pid: self.next_pid,
            boot_ms,
        };
        self.next_pid += 1;
        let mut memory = MemoryImage::new();
        memory.fill(0, 64, 0x7f); // kernel + init pages
        self.vms.insert(
            config.tab,
            SimVm {
                handle,
                config: config.clone(),
                paused: false,
                stats: VmStats {
                    resident_mb: 128,
                    ..VmStats::default()
                },
                memory,
                tick: 0,
                balloon_mb: 0,
            },
        );
        Ok(handle)
    }
}

/// A real backend that cannot run on this host, used to prove the trait shape
/// and to give the engine a truthful "not available here" answer.
#[derive(Debug, Default)]
pub struct NullBackend;

impl SpacetimeVmm for NullBackend {
    fn backend_name(&self) -> &'static str {
        "null"
    }
    fn availability(&self) -> Result<(), VmmError> {
        Err(VmmError::Unavailable(
            "no hypervisor wired up yet; see PLAN.md phase 3",
        ))
    }
    fn boot(&mut self, _config: &VmConfig) -> Result<VmHandle, VmmError> {
        self.availability()?;
        unreachable!()
    }
    fn clone_from_zygote(
        &mut self,
        _zygote: &mut Zygote,
        _config: &VmConfig,
    ) -> Result<VmHandle, VmmError> {
        self.availability()?;
        unreachable!()
    }
    fn pause(&mut self, _handle: VmHandle) -> Result<(), VmmError> {
        self.availability()
    }
    fn resume(&mut self, _handle: VmHandle) -> Result<(), VmmError> {
        self.availability()
    }
    fn stop(&mut self, _handle: VmHandle) -> Result<(), VmmError> {
        self.availability()
    }
    fn snapshot(&mut self, _handle: VmHandle) -> Result<MemoryImage, VmmError> {
        self.availability()?;
        unreachable!()
    }
    fn restore(&mut self, _handle: VmHandle, _image: &MemoryImage) -> Result<(), VmmError> {
        self.availability()
    }
    fn stats(&self, _handle: VmHandle) -> Result<VmStats, VmmError> {
        self.availability()?;
        unreachable!()
    }
    fn list(&self) -> Vec<VmHandle> {
        Vec::new()
    }
}

/// Convenience: hash an image path so mount lists are content-addressed.
pub fn image_id(image: &str) -> Hash {
    spacetime_core::sha256(image.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(tab: u64, net: NetMode) -> VmConfig {
        VmConfig::new(TabId(tab), "test", "debian-trixie.wavefunction", net)
    }

    #[test]
    fn boot_measures_time_and_registers_vm() {
        let mut backend = SimulatedBackend::new();
        let handle = backend.boot(&cfg(1, NetMode::Airgapped)).unwrap();
        assert_eq!(handle.pid, 4001);
        assert_eq!(backend.list().len(), 1);
        assert!(backend.stats(handle).unwrap().resident_mb > 0);
    }

    #[test]
    fn double_boot_is_rejected() {
        let mut backend = SimulatedBackend::new();
        backend.boot(&cfg(1, NetMode::Airgapped)).unwrap();
        assert!(matches!(
            backend.boot(&cfg(1, NetMode::Airgapped)),
            Err(VmmError::BadState { .. })
        ));
    }

    #[test]
    fn ticking_dirties_pages_and_reclaims_balloon() {
        let mut backend = SimulatedBackend::new();
        let handle = backend.boot(&cfg(1, NetMode::HoneyDuct)).unwrap();
        for _ in 0..11 {
            backend.tick();
        }
        let stats = backend.stats(handle).unwrap();
        assert!(stats.dirty_pages > 0);
        assert_eq!(stats.balloon_reclaimed_mb, 8);
    }

    #[test]
    fn paused_vm_does_not_change() {
        let mut backend = SimulatedBackend::new();
        let handle = backend.boot(&cfg(2, NetMode::Airgapped)).unwrap();
        backend.tick();
        let before = backend.snapshot(handle).unwrap();
        backend.pause(handle).unwrap();
        for _ in 0..5 {
            backend.tick();
        }
        let after = backend.snapshot(handle).unwrap();
        assert_eq!(before.page_count(), after.page_count());
        assert_eq!(before, after);
    }

    #[test]
    fn snapshot_restore_round_trip() {
        let mut backend = SimulatedBackend::new();
        let handle = backend.boot(&cfg(3, NetMode::Stealth)).unwrap();
        backend.tick();
        let saved = backend.snapshot(handle).unwrap();
        let count = saved.page_count();
        for _ in 0..3 {
            backend.tick();
        }
        assert!(backend.snapshot(handle).unwrap().page_count() > count);
        backend.restore(handle, &saved).unwrap();
        assert_eq!(backend.snapshot(handle).unwrap().page_count(), count);
    }

    #[test]
    fn stopping_frees_the_slot() {
        let mut backend = SimulatedBackend::new();
        let handle = backend.boot(&cfg(4, NetMode::Airgapped)).unwrap();
        backend.stop(handle).unwrap();
        assert!(backend.list().is_empty());
        assert!(backend.stats(handle).is_err());
        assert!(backend.memory_of(TabId(4)).is_none());
    }

    #[test]
    fn null_backend_reports_unavailable() {
        let mut backend = NullBackend;
        assert!(backend.availability().is_err());
        assert!(backend.boot(&cfg(5, NetMode::Airgapped)).is_err());
    }
}
