const EHDR_SIZE: u32 = 52;
const PHDR_SIZE: u32 = 32;
const PHDR_COUNT: u16 = 1;
const SEGMENT_FILE_OFFSET: u32 = 0x1000;
const SEGMENT_ALIGN: u32 = 0x1000;
const PT_LOAD: u32 = 1;
const PF_RWX: u32 = 7;

/// Lays out an ELF32 i386 executable with one RWX PT_LOAD segment matching
/// what the NyxaraOS userland loader expects (see rust/src/elf.rs): the
/// entry point starts at USER_BASE and the whole payload is copied verbatim.
pub fn write_elf32(
    text: &[u8],
    rodata: &[u8],
    data: &[u8],
    entry: u32,
    base: u32,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(text.len() + rodata.len() + data.len());
    payload.extend_from_slice(text);
    payload.extend_from_slice(rodata);
    payload.extend_from_slice(data);

    let mut out = Vec::with_capacity(SEGMENT_FILE_OFFSET as usize + payload.len());

    out.extend_from_slice(&[
        0x7F,
        b'E',
        b'L',
        b'F',
        1, // EI_CLASS: ELFCLASS32
        1, // EI_DATA: little endian
        1, // EI_VERSION
        0, // EI_OSABI
        0, // EI_ABIVERSION
        0, 0, 0, 0, 0, 0, 0,
    ]);
    out.extend_from_slice(&2u16.to_le_bytes()); // e_type: ET_EXEC
    out.extend_from_slice(&3u16.to_le_bytes()); // e_machine: EM_386
    out.extend_from_slice(&1u32.to_le_bytes()); // e_version
    out.extend_from_slice(&entry.to_le_bytes());
    out.extend_from_slice(&(EHDR_SIZE as u32).to_le_bytes()); // e_phoff
    out.extend_from_slice(&0u32.to_le_bytes()); // e_shoff
    out.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    out.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&(PHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&PHDR_COUNT.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    out.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    out.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    let filesz = payload.len() as u32;
    out.extend_from_slice(&PT_LOAD.to_le_bytes());
    out.extend_from_slice(&SEGMENT_FILE_OFFSET.to_le_bytes());
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&filesz.to_le_bytes());
    out.extend_from_slice(&filesz.to_le_bytes());
    out.extend_from_slice(&PF_RWX.to_le_bytes());
    out.extend_from_slice(&SEGMENT_ALIGN.to_le_bytes());

    out.resize(SEGMENT_FILE_OFFSET as usize, 0);
    out.extend_from_slice(&payload);
    out
}
