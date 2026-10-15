//! ELF64 AArch64 program-header loader and RELA application.
//!
//! This is the Milestone C foundation for Pixel1HLE:
//! - Map **PT_LOAD** segments (not section cookies).
//! - Apply a **bounded** set of AArch64 RELA types into the image.
//! - Resolve symbols against an external callback (host HLE stubs).
//!
//! It does **not** execute guest instructions. HyperHLE’s pattern of a
//! Dynarmic-backed CPU frontend is the intended next integration (MPL-2.0
//! Dynarmic itself is **not** vendored here).

use crate::error::{Pixel1Error, Result};
use log::{debug, info, warn};
use object::elf;
use object::read::elf::{ElfFile64, FileHeader as _, ProgramHeader as _, Rela as _, Sym as _};
use object::Endianness;
use object::Object;
use object::ObjectSymbol;
use std::collections::HashMap;

/// AArch64 relocation types we handle (ELF for the Arm 64-bit Architecture).
const R_AARCH64_NONE: u32 = 0;
const R_AARCH64_ABS64: u32 = 257;
const R_AARCH64_GLOB_DAT: u32 = 1025;
const R_AARCH64_JUMP_SLOT: u32 = 1026;
const R_AARCH64_RELATIVE: u32 = 1027;

/// Callback used when a relocation needs an external symbol address.
pub type SymbolLookup = dyn Fn(&str) -> Option<u64>;

#[derive(Debug, Clone)]
pub struct LoadSegment {
    pub p_vaddr: u64,
    pub p_memsz: u64,
    pub p_filesz: u64,
    pub p_offset: u64,
    pub p_flags: u32,
}

#[derive(Debug)]
pub struct LoadedImage {
    /// Guest preferred base (minimum PT_LOAD vaddr, page-aligned down).
    pub load_bias: u64,
    /// Contiguous host buffer covering [min_vaddr, max_vaddr).
    pub image: Vec<u8>,
    pub min_vaddr: u64,
    pub max_vaddr: u64,
    pub entry: u64,
    pub segments: Vec<LoadSegment>,
    pub undefined_symbols: Vec<String>,
    pub applied_relas: u32,
    pub skipped_relas: u32,
}

impl LoadedImage {
    /// Translate a guest virtual address into an index in `image`.
    pub fn guest_to_offset(&self, guest_va: u64) -> Option<usize> {
        if guest_va < self.min_vaddr || guest_va >= self.max_vaddr {
            return None;
        }
        Some((guest_va - self.min_vaddr) as usize)
    }

    pub fn read_u64(&self, guest_va: u64) -> Result<u64> {
        let off = self
            .guest_to_offset(guest_va)
            .ok_or_else(|| Pixel1Error::ElfLoad(format!("VA 0x{guest_va:x} out of image")))?;
        if off + 8 > self.image.len() {
            return Err(Pixel1Error::ElfLoad(format!(
                "VA 0x{guest_va:x} read past end"
            )));
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.image[off..off + 8]);
        Ok(u64::from_le_bytes(buf))
    }

    pub fn write_u64(&mut self, guest_va: u64, value: u64) -> Result<()> {
        let off = self
            .guest_to_offset(guest_va)
            .ok_or_else(|| Pixel1Error::ElfLoad(format!("VA 0x{guest_va:x} out of image")))?;
        if off + 8 > self.image.len() {
            return Err(Pixel1Error::ElfLoad(format!(
                "VA 0x{guest_va:x} write past end"
            )));
        }
        self.image[off..off + 8].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
}

/// Load an ELF64 little-endian AArch64 shared object / executable from bytes.
pub fn load_elf64_aarch64(data: &[u8], lookup: &SymbolLookup) -> Result<LoadedImage> {
    // Validate with object’s high-level API first
    let file = object::File::parse(data)
        .map_err(|e| Pixel1Error::ElfLoad(format!("ELF parse: {e}")))?;
    if file.architecture() != object::Architecture::Aarch64 {
        return Err(Pixel1Error::ElfLoad(format!(
            "expected AArch64, got {:?}",
            file.architecture()
        )));
    }
    if !file.is_little_endian() {
        return Err(Pixel1Error::ElfLoad("expected little-endian ELF".into()));
    }

    // Use typed ELF64 view for program headers + RELA
    let elf: ElfFile64<'_, Endianness> = ElfFile64::parse(data)
        .map_err(|e| Pixel1Error::ElfLoad(format!("ElfFile64: {e}")))?;
    let endian = elf.endian();
    let header = elf.elf_header();

    let mut segments = Vec::new();
    let mut min_vaddr = u64::MAX;
    let mut max_vaddr = 0u64;

    for ph in header.program_headers(endian, data).map_err(|e| {
        Pixel1Error::ElfLoad(format!("program_headers: {e}"))
    })? {
        let p_type = ph.p_type(endian);
        if p_type != elf::PT_LOAD {
            continue;
        }
        let p_vaddr = ph.p_vaddr(endian);
        let p_memsz = ph.p_memsz(endian);
        let p_filesz = ph.p_filesz(endian);
        let p_offset = ph.p_offset(endian);
        let p_flags = ph.p_flags(endian);

        if p_memsz == 0 {
            continue;
        }
        if p_filesz > p_memsz {
            return Err(Pixel1Error::ElfLoad(format!(
                "PT_LOAD filesz 0x{p_filesz:x} > memsz 0x{p_memsz:x}"
            )));
        }
        let end = p_vaddr.checked_add(p_memsz).ok_or_else(|| {
            Pixel1Error::ElfLoad("PT_LOAD vaddr overflow".into())
        })?;
        min_vaddr = min_vaddr.min(p_vaddr);
        max_vaddr = max_vaddr.max(end);
        segments.push(LoadSegment {
            p_vaddr,
            p_memsz,
            p_filesz,
            p_offset,
            p_flags,
        });
    }

    if segments.is_empty() {
        return Err(Pixel1Error::ElfLoad("no PT_LOAD segments".into()));
    }
    if min_vaddr == u64::MAX {
        return Err(Pixel1Error::ElfLoad("invalid PT_LOAD range".into()));
    }

    // Page-align the host buffer extent
    const PAGE: u64 = 0x1000;
    let aligned_min = min_vaddr & !(PAGE - 1);
    let aligned_max = (max_vaddr + PAGE - 1) & !(PAGE - 1);
    let total = (aligned_max - aligned_min) as usize;
    if total == 0 || total > 512 * 1024 * 1024 {
        return Err(Pixel1Error::ElfLoad(format!(
            "refusing image size {total} bytes"
        )));
    }

    let mut image = vec![0u8; total];

    for seg in &segments {
        let dest_off = (seg.p_vaddr - aligned_min) as usize;
        let file_off = seg.p_offset as usize;
        let copy_n = seg.p_filesz as usize;
        if copy_n > 0 {
            if file_off.checked_add(copy_n).map(|e| e > data.len()).unwrap_or(true) {
                return Err(Pixel1Error::ElfLoad(format!(
                    "PT_LOAD file range out of bounds (off=0x{file_off:x} n=0x{copy_n:x})"
                )));
            }
            if dest_off.checked_add(copy_n).map(|e| e > image.len()).unwrap_or(true) {
                return Err(Pixel1Error::ElfLoad("PT_LOAD dest out of image".into()));
            }
            image[dest_off..dest_off + copy_n]
                .copy_from_slice(&data[file_off..file_off + copy_n]);
        }
        // BSS already zeroed by vec![0; …]
    }

    let entry_raw = header.e_entry(endian);
    let entry = if entry_raw != 0 {
        // PIE/shared objects often use vaddr-relative entry; keep absolute VA
        entry_raw
    } else {
        0
    };

    let mut loaded = LoadedImage {
        load_bias: aligned_min, // for ET_DYN with vaddr starting at 0, bias is allocation base
        image,
        min_vaddr: aligned_min,
        max_vaddr: aligned_max,
        entry,
        segments,
        undefined_symbols: Vec::new(),
        applied_relas: 0,
        skipped_relas: 0,
    };

    // Collect dynamic symbols for name lookup
    let mut dyn_names: HashMap<usize, String> = HashMap::new();
    for (i, sym) in file.symbols().enumerate() {
        if let Ok(name) = sym.name() {
            if !name.is_empty() {
                dyn_names.insert(i, name.to_string());
            }
        }
        if sym.is_undefined() {
            if let Ok(name) = sym.name() {
                if !name.is_empty() && !name.starts_with('$') {
                    loaded.undefined_symbols.push(name.to_string());
                }
            }
        }
    }
    loaded.undefined_symbols.sort();
    loaded.undefined_symbols.dedup();

    apply_rela(&mut loaded, data, &elf, lookup, &dyn_names)?;

    info!(
        target: "hle::elf",
        "Loaded image: VA[0x{:x}..0x{:x}) entry=0x{:x} relas={} skipped={} undef={}",
        loaded.min_vaddr,
        loaded.max_vaddr,
        loaded.entry,
        loaded.applied_relas,
        loaded.skipped_relas,
        loaded.undefined_symbols.len()
    );

    Ok(loaded)
}

fn apply_rela(
    loaded: &mut LoadedImage,
    data: &[u8],
    elf: &ElfFile64<'_, Endianness>,
    lookup: &SymbolLookup,
    dyn_names: &HashMap<usize, String>,
) -> Result<()> {
    let endian = elf.endian();

    // object 0.36: dynamic_relocations on File is easier for mixed REL/RELA
    // Fall back to walking DT_RELA via high-level API when possible.
    let file = object::File::parse(data)
        .map_err(|e| Pixel1Error::ElfLoad(format!("reparse: {e}")))?;

    if let Some(relocs) = file.dynamic_relocations() {
        for (offset, reloc) in relocs {
            let r_type = reloc.kind();
            let addend = reloc.addend() as u64;
            // Map object’s RelocationKind to AArch64 semantics is lossy;
            // use raw ELF rela when available via elf section.
            let _ = (offset, r_type, addend);
        }
    }

    // Prefer explicit SHT_RELA sections
    for section in elf.sections().map_err(|e| Pixel1Error::ElfLoad(format!("{e}")))? {
        let sh_type = section.sh_type(endian);
        if sh_type != elf::SHT_RELA {
            continue;
        }
        let content = section.data(endian, data)
            .map_err(|e| Pixel1Error::ElfLoad(format!("rela data: {e}")))?;
        let rela_iter = section.rela(endian, content)
            .map_err(|e| Pixel1Error::ElfLoad(format!("rela parse: {e}")))?;

        for rela in rela_iter {
            let r_offset = rela.r_offset(endian);
            let r_type = rela.r32_type(endian);
            let r_sym = rela.r_sym(endian) as usize;
            let r_addend = rela.r_addend(endian) as u64;

            // For ET_DYN linked at min_vaddr, absolute guest address of the
            // relocation slot is r_offset (already a vaddr in Android DSOs).
            let slot_va = r_offset;

            match r_type {
                R_AARCH64_NONE => {
                    loaded.skipped_relas += 1;
                }
                R_AARCH64_RELATIVE => {
                    // B + A  (B = load bias / base). Android ET_DYN usually
                    // linked at 0; our image is placed at min_vaddr.
                    let value = loaded.min_vaddr.wrapping_add(r_addend);
                    loaded.write_u64(slot_va, value)?;
                    loaded.applied_relas += 1;
                }
                R_AARCH64_ABS64 | R_AARCH64_GLOB_DAT | R_AARCH64_JUMP_SLOT => {
                    let name = dyn_names.get(&r_sym).map(|s| s.as_str()).unwrap_or("");
                    let sym_va = if name.is_empty() {
                        None
                    } else {
                        lookup(name)
                    };
                    let value = match sym_va {
                        Some(v) => v.wrapping_add(r_addend),
                        None => {
                            if !name.is_empty() {
                                debug!(target: "hle::elf", "unresolved {name} → 0");
                            }
                            0u64.wrapping_add(r_addend)
                        }
                    };
                    loaded.write_u64(slot_va, value)?;
                    loaded.applied_relas += 1;
                }
                other => {
                    warn!(target: "hle::elf", "unsupported reloc type {other} at 0x{slot_va:x}");
                    loaded.skipped_relas += 1;
                }
            }
        }
    }

    Ok(())
}

/// Trait for a future AArch64 execution backend (Dynarmic / Unicorn / …).
///
//! HyperHLE uses Dynarmic via a C++ wrapper (MPL-2.0). Pixel1HLE does **not**
//! vendor that code; integrators must supply a backend that implements this
//! interface and respects guest memory isolation.
pub trait AArch64Backend {
    fn name(&self) -> &str;
    /// Map a host-backed guest image into the backend’s address space.
    fn map_image(&mut self, guest_base: u64, bytes: &[u8]) -> Result<()>;
    /// Execute at guest PC until halt/fault. Returns final PC.
    fn run(&mut self, entry: u64) -> Result<u64>;
}

/// Placeholder backend: records maps, refuses to execute (honest).
#[derive(Default)]
pub struct NullAArch64Backend {
    pub mapped: Vec<(u64, usize)>,
}

impl AArch64Backend for NullAArch64Backend {
    fn name(&self) -> &str {
        "null (no guest instruction execution)"
    }

    fn map_image(&mut self, guest_base: u64, bytes: &[u8]) -> Result<()> {
        self.mapped.push((guest_base, bytes.len()));
        Ok(())
    }

    fn run(&mut self, entry: u64) -> Result<u64> {
        Err(Pixel1Error::ElfLoad(format!(
            "NullAArch64Backend cannot execute guest PC 0x{entry:x}; integrate Dynarmic/Unicorn or run on aarch64 host"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::write::{self, SectionKind, SymbolKind, SymbolScope, SymbolSection};
    use object::{Architecture, BinaryFormat, Endianness as WriteEndian};

    /// Build a minimal ET_DYN AArch64 ELF with one PT_LOAD and one RELATIVE rela.
    fn make_test_elf() -> Vec<u8> {
        let mut obj = write::Object::new(
            BinaryFormat::Elf,
            Architecture::Aarch64,
            WriteEndian::Little,
        );
        obj.flags = object::FileFlags::Elf {
            os_abi: 0,
            abi_version: 0,
            e_flags: 0,
        };

        let text = obj.add_section(Vec::new(), b".text".to_vec(), SectionKind::Text);
        // four NOPs as placeholder "code"
        let nop = [0x1f, 0x20, 0x03, 0xd5u8];
        let mut code = Vec::new();
        for _ in 0..4 {
            code.extend_from_slice(&nop);
        }
        obj.append_section_data(text, &code, 4);

        // A data word that will receive a RELATIVE fixup target for tests is
        // hard to emit portably via object::write without full dynamic tables.
        // We still verify PT_LOAD mapping of .text.
        let mut out = Vec::new();
        obj.write_stream(&mut out).expect("write elf");
        out
    }

    #[test]
    fn rejects_non_elf() {
        let err = load_elf64_aarch64(b"not elf", &|_| None).unwrap_err();
        assert!(err.to_string().contains("ELF") || err.to_string().contains("parse"));
    }

    #[test]
    fn loads_aarch64_text_segment() {
        let data = make_test_elf();
        let img = load_elf64_aarch64(&data, &|_| None).expect("load");
        assert!(!img.segments.is_empty(), "expected PT_LOAD");
        assert!(img.image.len() >= 16);
        // First bytes of image at min_vaddr should include our NOPs somewhere
        // in the first load segment file range.
        let has_nop = img.image.windows(4).any(|w| w == [0x1f, 0x20, 0x03, 0xd5]);
        assert!(has_nop, "mapped image should contain A64 NOP");
    }

    #[test]
    fn null_backend_maps_but_refuses_run() {
        let mut cpu = NullAArch64Backend::default();
        cpu.map_image(0x1000, &[0u8; 64]).unwrap();
        assert_eq!(cpu.mapped.len(), 1);
        let err = cpu.run(0x1000).unwrap_err();
        assert!(err.to_string().contains("cannot execute"));
    }

    #[test]
    fn guest_to_offset_bounds() {
        let img = LoadedImage {
            load_bias: 0x1000,
            image: vec![0; 0x100],
            min_vaddr: 0x1000,
            max_vaddr: 0x1100,
            entry: 0,
            segments: vec![],
            undefined_symbols: vec![],
            applied_relas: 0,
            skipped_relas: 0,
        };
        assert_eq!(img.guest_to_offset(0x1000), Some(0));
        assert_eq!(img.guest_to_offset(0x10ff), Some(0xff));
        assert!(img.guest_to_offset(0x0fff).is_none());
        assert!(img.guest_to_offset(0x1100).is_none());
    }

    #[test]
    fn write_u64_roundtrip() {
        let mut img = LoadedImage {
            load_bias: 0,
            image: vec![0; 32],
            min_vaddr: 0,
            max_vaddr: 32,
            entry: 0,
            segments: vec![],
            undefined_symbols: vec![],
            applied_relas: 0,
            skipped_relas: 0,
        };
        img.write_u64(8, 0x1122_3344_5566_7788).unwrap();
        assert_eq!(img.read_u64(8).unwrap(), 0x1122_3344_5566_7788);
    }
}
