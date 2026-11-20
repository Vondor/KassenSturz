// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin host (#791, D-007, D-753, D-850): loads a plugin file and
//! runs its WebAssembly in a sandbox.
//!
//! The runtime is wasmi, an interpreter, so no compiler comes into the app
//! (Q-633). A plugin gets nothing it isn't given:
//!
//! - **No imports.** API v1 step 2 offers no host function, so a module that
//!   imports anything is refused. Later steps add functions in the
//!   `kassensturz` module, each behind a permission of the manifest (D-848).
//!   There is no WASI: no files, clock, randomness or network.
//! - **Bytes in, bytes out.** The host copies the input into the plugin's
//!   memory and copies the output out of it; the plugin never sees the
//!   host's memory.
//! - **Limits.** One memory of at most [`Limits::memory_bytes`], a bounded
//!   table, and fuel for each call, so a plugin that loops forever is
//!   stopped ([`HostError::RanTooLong`]).
//! - **One strike.** After a trap or running out of fuel the plugin's state
//!   can't be trusted, so every later call is refused until it is loaded
//!   again.
//! - **AI off means not loaded** (D-013, D-754).
//!
//! The calling convention is in `docs/PLUGINS.md` §5.

mod error;

pub use error::HostError;

use kassensturz_plugin_api::{Manifest, Package};
use wasmi::{
    Config, Engine, ExternType, Instance, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TrapCode, TypedFunc,
};

/// The export a plugin gives the host room through: `ks_alloc(len) -> ptr`.
pub const ALLOC_EXPORT: &str = "ks_alloc";
/// The memory a plugin exports for the host to copy bytes in and out.
pub const MEMORY_EXPORT: &str = "memory";

/// The sandbox's limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most bytes the plugin's memory may grow to.
    pub memory_bytes: usize,
    /// The most elements of its table.
    pub table_elements: usize,
    /// The fuel of one call, and of the module's start function. One unit
    /// is about one instruction.
    pub fuel_per_call: u64,
    /// The most bytes the host passes in.
    pub max_input_bytes: usize,
    /// The most bytes the host takes back.
    pub max_output_bytes: usize,
}

impl Default for Limits {
    /// 64 MiB of memory, 10 000 table elements, a billion units of fuel per
    /// call (a few seconds on a slow computer) and 16 MiB in and out.
    fn default() -> Self {
        Self {
            memory_bytes: 64 * 1024 * 1024,
            table_elements: 10_000,
            fuel_per_call: 1_000_000_000,
            max_input_bytes: 16 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
        }
    }
}

/// A loaded plugin.
pub struct Plugin {
    manifest: Manifest,
    limits: Limits,
    code: Option<Code>,
    stopped: bool,
}

/// The running module of a plugin with code.
struct Code {
    module: Module,
    store: Store<StoreLimits>,
    instance: Instance,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
}

impl core::fmt::Debug for Plugin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Plugin")
            .field("id", &self.manifest.id)
            .field("has_code", &self.code.is_some())
            .field("stopped", &self.stopped)
            .finish()
    }
}

impl Plugin {
    /// Loads a plugin file with the default limits. `ai_enabled` is the
    /// "AI features" switch: while it is off, an AI plugin is never loaded.
    pub fn load(bytes: &[u8], ai_enabled: bool) -> Result<Self, HostError> {
        Self::load_with(bytes, ai_enabled, Limits::default())
    }

    /// Loads a plugin file with the given limits.
    pub fn load_with(bytes: &[u8], ai_enabled: bool, limits: Limits) -> Result<Self, HostError> {
        let Package {
            manifest, has_code, ..
        } = Package::read(bytes)?;
        if manifest.is_ai() && !ai_enabled {
            return Err(HostError::AiOff);
        }
        let code = if has_code || !manifest.is_data_only() {
            Some(Code::start(bytes, limits)?)
        } else {
            None
        };
        Ok(Self {
            manifest,
            limits,
            code,
            stopped: false,
        })
    }

    /// The plugin's checked manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Whether it was stopped by a trap or by running too long, so that it
    /// takes no more calls.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Calls the export `name(ptr: i32, len: i32) -> i64` with `input` and
    /// returns its output: the high 32 bits of the result are the output's
    /// address in the plugin's memory, the low 32 bits its length.
    pub fn call(&mut self, name: &str, input: &[u8]) -> Result<Vec<u8>, HostError> {
        if self.stopped {
            return Err(HostError::Stopped);
        }
        let code = self.code.as_mut().ok_or(HostError::NoCode)?;
        let result = code.call(name, input, self.limits);
        if matches!(
            result,
            Err(HostError::RanTooLong | HostError::Trapped | HostError::InputNotAccepted)
        ) {
            self.stopped = true;
        }
        result
    }
}

impl Code {
    fn start(bytes: &[u8], limits: Limits) -> Result<Self, HostError> {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(|_| HostError::ModuleInvalid)?;
        if let Some(import) = module.imports().next() {
            return Err(HostError::ImportNotAllowed {
                module: import.module().to_owned(),
                name: import.name().to_owned(),
            });
        }
        let store_limits = StoreLimitsBuilder::new()
            .memory_size(limits.memory_bytes)
            .table_elements(limits.table_elements)
            .instances(1)
            .memories(1)
            .tables(1)
            .build();
        let mut store = Store::new(&engine, store_limits);
        store.limiter(|limits| limits);
        store
            .set_fuel(limits.fuel_per_call)
            .map_err(|_| HostError::ModuleInvalid)?;
        let linker = Linker::<StoreLimits>::new(&engine);
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|error| match error.as_trap_code() {
                Some(TrapCode::OutOfFuel) => HostError::RanTooLong,
                _ => HostError::StartFailed,
            })?;
        let memory = match module.get_export(MEMORY_EXPORT) {
            Some(ExternType::Memory(_)) => {
                instance
                    .get_memory(&store, MEMORY_EXPORT)
                    .ok_or(HostError::ExportMissing {
                        name: MEMORY_EXPORT.to_owned(),
                    })?
            }
            Some(_) => {
                return Err(HostError::ExportWrongType {
                    name: MEMORY_EXPORT.to_owned(),
                });
            }
            None => {
                return Err(HostError::ExportMissing {
                    name: MEMORY_EXPORT.to_owned(),
                });
            }
        };
        let alloc = typed_func::<i32, i32>(&module, &instance, &store, ALLOC_EXPORT)?;
        Ok(Self {
            module,
            store,
            instance,
            memory,
            alloc,
        })
    }

    fn call(&mut self, name: &str, input: &[u8], limits: Limits) -> Result<Vec<u8>, HostError> {
        if input.len() > limits.max_input_bytes {
            return Err(HostError::InputTooBig {
                max_bytes: limits.max_input_bytes,
            });
        }
        let func = typed_func::<(i32, i32), i64>(&self.module, &self.instance, &self.store, name)?;
        self.store
            .set_fuel(limits.fuel_per_call)
            .map_err(|_| HostError::Trapped)?;
        let length = i32::try_from(input.len()).map_err(|_| HostError::InputTooBig {
            max_bytes: limits.max_input_bytes,
        })?;
        let address = self.alloc.call(&mut self.store, length).map_err(trap)?;
        let start = usize::try_from(address).map_err(|_| HostError::InputNotAccepted)?;
        self.memory
            .write(&mut self.store, start, input)
            .map_err(|_| HostError::InputNotAccepted)?;
        let packed = func
            .call(&mut self.store, (address, length))
            .map_err(trap)?;
        // The high half is the address, the low half the length; both are
        // read as unsigned 32-bit numbers.
        let packed = u64::from_ne_bytes(packed.to_ne_bytes());
        let out_start = usize::try_from(packed >> 32).map_err(|_| HostError::OutputInvalid)?;
        let out_length =
            usize::try_from(packed & 0xFFFF_FFFF).map_err(|_| HostError::OutputInvalid)?;
        if out_length > limits.max_output_bytes {
            return Err(HostError::OutputTooBig {
                max_bytes: limits.max_output_bytes,
            });
        }
        let mut output = vec![0; out_length];
        self.memory
            .read(&self.store, out_start, &mut output)
            .map_err(|_| HostError::OutputInvalid)?;
        Ok(output)
    }
}

fn trap(error: wasmi::Error) -> HostError {
    match error.as_trap_code() {
        Some(TrapCode::OutOfFuel) => HostError::RanTooLong,
        _ => HostError::Trapped,
    }
}

/// The export `name` as a function of the given type, or why not.
fn typed_func<Params, Results>(
    module: &Module,
    instance: &Instance,
    store: &Store<StoreLimits>,
    name: &str,
) -> Result<TypedFunc<Params, Results>, HostError>
where
    Params: wasmi::WasmParams,
    Results: wasmi::WasmResults,
{
    match module.get_export(name) {
        None => Err(HostError::ExportMissing {
            name: name.to_owned(),
        }),
        Some(ExternType::Func(_)) => instance
            .get_typed_func::<Params, Results>(store, name)
            .map_err(|_| HostError::ExportWrongType {
                name: name.to_owned(),
            }),
        Some(_) => Err(HostError::ExportWrongType {
            name: name.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests;
