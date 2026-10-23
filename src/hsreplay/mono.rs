//! Reads the player's collection out of the running game.
//!
//! Hearthstone is a Unity game on the Mono runtime, so its C# objects can be
//! found by walking Mono's own data structures through `/proc/<pid>/mem`,
//! the way HearthMirror does on Windows. Nothing is written and nothing is
//! injected.
//!
//! Reading another process's memory needs ptrace access. With the common
//! `kernel.yama.ptrace_scope = 1` that is only granted to an ancestor of the
//! target, which is why this only works for a game the launcher started
//! itself (directly or through wrappers such as `gamemoderun`).
//!
//! Struct offsets were found on Unity 6000.3 / Linux x86_64. They are close to
//! UnitySpy's `Unity2021_3_2022_3_x64` table, except that `MonoClass` fields
//! sit 8 bytes lower than in the Windows build. They are not discovered at
//! run time, so a different Unity version (or another architecture) makes the
//! reads fail or find nothing; this is best-effort and stops with an error
//! rather than guessing. Every walk over Mono's linked structures is capped, so
//! corrupt or half-built memory ends in an error, never in an endless loop.

use anyhow::{bail, ensure, Context, Result};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs::{self, File},
    os::unix::fs::FileExt,
};

const DOMAIN_ASSEMBLIES: u64 = 0x98;
const ASSEMBLY_NAME: u64 = 0x10;
const ASSEMBLY_IMAGE: u64 = 0x60;
const IMAGE_CLASS_CACHE: u64 = 0x4D0;
const HASH_TABLE_SIZE: u64 = 0x18;
const HASH_TABLE_TABLE: u64 = 0x20;
const CLASS_PARENT: u64 = 0x28;
const CLASS_NAME: u64 = 0x40;
const CLASS_NAMESPACE: u64 = 0x48;
const CLASS_VTABLE_SIZE: u64 = 0x54;
const CLASS_FIELDS: u64 = 0x90;
const CLASS_RUNTIME_INFO: u64 = 0xC8;
const CLASS_FIELD_COUNT: u64 = 0xF8;
const CLASS_NEXT_CACHE: u64 = 0x100;
const FIELD_SIZE: u64 = 0x20;
const VTABLE_VTABLE: u64 = 0x48;
const FIELD_ATTRIBUTE_STATIC: u16 = 0x10;

/// Upper bounds for walks over data read from the game, so corrupt memory
/// cannot make them run forever.
const MAX_ASSEMBLIES: usize = 4096;
const MAX_CLASS_BUCKETS: u64 = 1 << 20;
const MAX_CLASSES_VISITED: usize = 1 << 21;
const MAX_CLASS_CHAIN: usize = 64;

const GAME_EXE: &str = "Hearthstone.x86_64";
const MONO_LIBRARY: &str = "libmonobdwgc-2.0.so";

/// One entry of the in-game collection: a card in one finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub dbf_id: i32,
    /// 0 normal, 1 golden, 2 diamond, 3 signature.
    pub premium: i32,
    pub count: i32,
    pub trial: i32,
}

/// What the HSReplay upload needs from the game.
#[derive(Clone, Debug)]
pub struct GameState {
    pub account_hi: u64,
    pub account_lo: u64,
    pub battletag: Option<String>,
    pub dust: Option<u64>,
    pub cards: Vec<Card>,
}

impl GameState {
    pub fn owned_copies(&self) -> i64 {
        self.cards.iter().map(|card| i64::from(card.count)).sum()
    }
}

/// The game process among `root` and its descendants, if it is running.
pub fn find_game(root: u32) -> Option<u32> {
    let pids = fs::read_dir("/proc")
        .ok()?
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok());
    for pid in pids {
        let is_game = fs::read_link(format!("/proc/{pid}/exe"))
            .is_ok_and(|exe| exe.file_name().is_some_and(|name| name == GAME_EXE));
        if is_game && descends_from(pid, root) {
            return Some(pid);
        }
    }
    None
}

/// Whether `pid` exists and has not exited (a zombie that nobody has waited
/// for yet counts as exited).
pub fn process_alive(pid: u32) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    stat.rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .is_some_and(|state| !matches!(state, "Z" | "X"))
}

fn descends_from(mut pid: u32, root: u32) -> bool {
    // Process trees are shallow; the cap only guards against a cycle.
    for _ in 0..4096 {
        if pid <= 1 {
            return false;
        }
        if pid == root {
            return true;
        }
        match parent_pid(pid) {
            Some(parent) => pid = parent,
            None => return false,
        }
    }
    false
}

fn parent_pid(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // "pid (comm) state ppid ...": comm may contain spaces and parentheses.
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// Reads the collection, account and dust of the game process `pid`.
/// Fails while the game is still loading.
pub fn read_state(pid: u32) -> Result<GameState> {
    let mem = Mem::open(pid)?;
    let domain = root_domain(&mem)?;
    let image = find_image(&mem, domain, "Assembly-CSharp")?;
    let classes = find_classes(
        &mem,
        image,
        &[
            "CollectionManager",
            "BnetPresenceMgr",
            "NetCacheArcaneDustBalance",
        ],
    )?;

    let collection = mem.static_ref(classes["CollectionManager"], "s_instance")?;
    ensure!(collection != 0, "CollectionManager is not ready yet");
    let mut cards = Vec::new();
    for card in mem.list_items(mem.get(collection, "m_collectibleCards")?)? {
        cards.push(Card {
            dbf_id: mem.get_i32(card, "m_CardDbId")?,
            premium: mem.get_i32(card, "m_PremiumType")?,
            count: mem.get_i32(card, "<OwnedCount>k__BackingField")?,
            trial: mem.get_i32(card, "<TrialCount>k__BackingField")?,
        });
    }

    let presence = mem.static_ref(classes["BnetPresenceMgr"], "s_instance")?;
    ensure!(presence != 0, "BnetPresenceMgr is not ready yet");
    let account = mem.get(presence, "m_myGameAccountId")?;
    let entity = mem.get(account, "<EntityId>k__BackingField")?;
    let account_hi = mem.get_u64(entity, "high_")?;
    let account_lo = mem.get_u64(entity, "low_")?;
    // Only needed to link the account on HSReplay, so a failure is not fatal.
    let battletag = read_battletag(&mem, presence)
        .inspect_err(|error| tracing::warn!(error = %format!("{error:#}"), "battletag not read"))
        .ok();
    // Dust is optional: when it cannot be read, the collection is still sent.
    let dust = read_dust(&mem, classes["NetCacheArcaneDustBalance"])
        .inspect_err(|error| tracing::warn!(error = %format!("{error:#}"), "dust not read"))
        .ok()
        .flatten();

    Ok(GameState {
        account_hi,
        account_lo,
        battletag,
        dust,
        cards,
    })
}

fn read_battletag(mem: &Mem, presence: u64) -> Result<String> {
    let player = mem.get(presence, "m_myPlayer")?;
    let tag = mem.get(mem.get(player, "m_account")?, "m_battleTag")?;
    let name = mem.mono_string(mem.get(tag, "m_name")?)?;
    let number = mem.mono_string(mem.get(tag, "m_number")?)?;
    Ok(format!("{name}#{number}"))
}

/// Finds `NetCacheArcaneDustBalance` objects by scanning writable memory for
/// pointers to the class vtable.
// ponytail: heap scan (~3 s) instead of walking ServiceManager -> NetCache ->
// Map<Type, object>; walk that path if stale duplicates ever show up.
fn read_dust(mem: &Mem, class: u64) -> Result<Option<u64>> {
    const CHUNK: u64 = 1 << 24;
    let vtable = mem.ptr(mem.ptr(class + CLASS_RUNTIME_INFO)? + 8)?;
    let balance = mem.field(class, "<Balance>k__BackingField")?.0;
    let maps = fs::read_to_string(format!("/proc/{}/maps", mem.pid))?;
    let mut found = HashSet::new();
    for line in maps.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        let Some((start, end)) = parse_range(parts[0]) else {
            continue;
        };
        let skip = !parts[1].starts_with("rw")
            || end - start > 1 << 31
            || parts.get(5).is_some_and(|path| path.starts_with("/dev"));
        if skip {
            continue;
        }
        let mut chunk_start = start;
        while chunk_start < end {
            let len = CHUNK.min(end - chunk_start);
            if let Ok(chunk) = mem.read(chunk_start, len as usize) {
                for (index, word) in chunk.chunks_exact(8).enumerate() {
                    if u64::from_le_bytes(word.try_into().unwrap()) != vtable {
                        continue;
                    }
                    let object = chunk_start + 8 * index as u64;
                    // A live managed object has an empty sync header and a sane balance.
                    let header_empty = mem.u64(object + 8).ok() == Some(0);
                    match mem.u64(object + balance) {
                        Ok(value) if header_empty && value < 10_000_000 => {
                            found.insert(value);
                        }
                        _ => {}
                    }
                }
            }
            chunk_start += len;
        }
    }
    Ok(single_dust(&found))
}

/// The dust balance, only when every plausible object agrees on it. Stale
/// copies on the heap can disagree; picking one of them (say the largest)
/// would upload a made-up balance, so an ambiguous scan reports nothing.
fn single_dust(found: &HashSet<u64>) -> Option<u64> {
    match found.len() {
        0 => {
            tracing::warn!("no dust balance found in the game's memory; not uploading dust");
            None
        }
        1 => found.iter().next().copied(),
        _ => {
            tracing::warn!(
                ?found,
                "several different dust balances found; not uploading dust"
            );
            None
        }
    }
}

fn parse_range(range: &str) -> Option<(u64, u64)> {
    let (start, end) = range.split_once('-')?;
    Some((
        u64::from_str_radix(start, 16).ok()?,
        u64::from_str_radix(end, 16).ok()?,
    ))
}

/// Address of Mono's root domain, read from the global that the exported
/// `mono_get_root_domain` returns.
fn root_domain(mem: &Mem) -> Result<u64> {
    let maps = fs::read_to_string(format!("/proc/{}/maps", mem.pid))?;
    let (base, path) = maps
        .lines()
        .find_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            let path = *parts.get(5)?;
            let first_segment = parts[2].trim_start_matches('0').is_empty();
            if !path.ends_with(MONO_LIBRARY) || !first_segment {
                return None;
            }
            Some((parse_range(parts[0])?.0, path))
        })
        .context("Mono runtime is not loaded yet")?;
    // Through /proc/<pid>/root so a runtime container's paths resolve too.
    let library = fs::read(format!("/proc/{}/root{path}", mem.pid))
        .with_context(|| format!("failed to read {path}"))?;
    let symbol = base + dynamic_symbol(&library, "mono_get_root_domain")?;

    // The function is `mov rax, [rip + rel32]; ret`, maybe after `endbr64`.
    let code = mem.read(symbol, 16)?;
    let start = if code.starts_with(&[0xf3, 0x0f, 0x1e, 0xfa]) {
        4
    } else {
        0
    };
    ensure!(
        code[start..start + 3] == [0x48, 0x8b, 0x05],
        "unexpected mono_get_root_domain code: {}",
        hex::encode(&code)
    );
    let relative = i32::from_le_bytes(code[start + 3..start + 7].try_into().unwrap());
    let global = (symbol + start as u64 + 7).wrapping_add_signed(relative.into());
    mem.ptr(global)
}

/// Value of an exported symbol in an ELF64 shared library.
fn dynamic_symbol(elf: &[u8], name: &str) -> Result<u64> {
    let u16_at = |at: usize| {
        elf.get(at..at + 2)
            .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
    };
    let u32_at = |at: usize| {
        elf.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let u64_at = |at: usize| {
        elf.get(at..at + 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    };
    ensure!(elf.starts_with(b"\x7fELF\x02"), "not an ELF64 file");

    let section_table = u64_at(0x28).context("truncated ELF header")? as usize;
    let section_size = usize::from(u16_at(0x3a).context("truncated ELF header")?);
    let section_count = usize::from(u16_at(0x3c).context("truncated ELF header")?);
    let section = |index: usize| section_table + index * section_size;
    for index in 0..section_count {
        const SHT_DYNSYM: u32 = 11;
        if u32_at(section(index) + 4) != Some(SHT_DYNSYM) {
            continue;
        }
        let symbols = u64_at(section(index) + 0x18).context("truncated section")? as usize;
        let symbols_size = u64_at(section(index) + 0x20).context("truncated section")? as usize;
        let strings_index = u32_at(section(index) + 0x28).context("truncated section")? as usize;
        let strings = u64_at(section(strings_index) + 0x18).context("truncated section")? as usize;
        for symbol in (symbols..symbols + symbols_size).step_by(24) {
            let name_at = strings + u32_at(symbol).context("truncated symbol")? as usize;
            let symbol_name = elf
                .get(name_at..)
                .and_then(|rest| rest.split(|&byte| byte == 0).next());
            if symbol_name == Some(name.as_bytes()) {
                return u64_at(symbol + 8).context("truncated symbol");
            }
        }
    }
    bail!("symbol {name} not found")
}

fn find_image(mem: &Mem, domain: u64, name: &str) -> Result<u64> {
    let mut node = mem.ptr(domain + DOMAIN_ASSEMBLIES)?;
    for _ in 0..MAX_ASSEMBLIES {
        if node == 0 {
            break;
        }
        let assembly = mem.ptr(node)?;
        if mem.cstr(mem.ptr(assembly + ASSEMBLY_NAME)?)?.as_deref() == Some(name) {
            return mem.ptr(assembly + ASSEMBLY_IMAGE);
        }
        node = mem.ptr(node + 8)?;
    }
    if node != 0 {
        bail!("assembly list is longer than {MAX_ASSEMBLIES}; memory looks corrupt");
    }
    bail!("assembly {name} is not loaded yet")
}

/// Classes without a namespace, looked up in the image's class cache.
fn find_classes(mem: &Mem, image: u64, names: &[&str]) -> Result<HashMap<String, u64>> {
    let cache = image + IMAGE_CLASS_CACHE;
    let size = mem.i32(cache + HASH_TABLE_SIZE)?;
    let table = mem.ptr(cache + HASH_TABLE_TABLE)?;
    let buckets = u64::try_from(size).unwrap_or(0);
    ensure!(
        buckets <= MAX_CLASS_BUCKETS,
        "class cache size {buckets} is implausible"
    );
    let mut found = HashMap::new();
    let mut visited = 0usize;
    for bucket in 0..buckets {
        let mut class = mem.ptr(table + 8 * bucket)?;
        while class != 0 {
            visited += 1;
            ensure!(
                visited <= MAX_CLASSES_VISITED,
                "class cache has too many entries; memory looks corrupt"
            );
            if let Some(name) = mem.cstr(mem.ptr(class + CLASS_NAME)?)? {
                let namespace = mem.cstr(mem.ptr(class + CLASS_NAMESPACE)?)?;
                if names.contains(&name.as_str()) && namespace.as_deref().unwrap_or("").is_empty() {
                    found.insert(name, class);
                }
            }
            class = mem.ptr(class + CLASS_NEXT_CACHE)?;
        }
    }
    let missing: Vec<_> = names
        .iter()
        .filter(|name| !found.contains_key(**name))
        .collect();
    ensure!(missing.is_empty(), "classes not found: {missing:?}");
    Ok(found)
}

/// A `(offset, is_static)` pair per field name, own fields before inherited ones.
type Fields = HashMap<String, (u64, bool)>;

struct Mem {
    pid: u32,
    file: File,
    fields: RefCell<HashMap<u64, Fields>>,
}

impl Mem {
    fn open(pid: u32) -> Result<Self> {
        let file = File::open(format!("/proc/{pid}/mem")).with_context(|| {
            format!("cannot read the memory of process {pid}; was it started by this launcher?")
        })?;
        Ok(Self {
            pid,
            file,
            fields: RefCell::default(),
        })
    }

    fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
        ensure!(address > 0 && address < 1 << 47, "bad address {address:#x}");
        let mut buffer = vec![0; len];
        self.file
            .read_exact_at(&mut buffer, address)
            .with_context(|| format!("failed to read {len} bytes at {address:#x}"))?;
        Ok(buffer)
    }

    fn array<const N: usize>(&self, address: u64) -> Result<[u8; N]> {
        Ok(self.read(address, N)?.try_into().unwrap())
    }

    fn ptr(&self, address: u64) -> Result<u64> {
        self.u64(address)
    }

    fn u64(&self, address: u64) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array(address)?))
    }

    fn i32(&self, address: u64) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array(address)?))
    }

    fn u16(&self, address: u64) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array(address)?))
    }

    fn cstr(&self, address: u64) -> Result<Option<String>> {
        if address == 0 {
            return Ok(None);
        }
        let bytes = self.read(address, 128)?;
        let end = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        Ok(Some(String::from_utf8_lossy(&bytes[..end]).into_owned()))
    }

    /// `System.String`: object header, i32 length, UTF-16 characters.
    fn mono_string(&self, object: u64) -> Result<String> {
        ensure!(object != 0, "null string");
        let len = usize::try_from(self.i32(object + 0x10)?).context("bad string length")?;
        ensure!(len < 1 << 16, "bad string length {len}");
        let bytes = self.read(object + 0x14, 2 * len)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }

    fn fields(&self, class: u64) -> Result<Fields> {
        if let Some(fields) = self.fields.borrow().get(&class) {
            return Ok(fields.clone());
        }
        let mut fields = Fields::new();
        let mut current = class;
        let mut depth = 0;
        while current != 0 {
            depth += 1;
            ensure!(
                depth <= MAX_CLASS_CHAIN,
                "class inheritance chain is too deep; memory looks corrupt"
            );
            let array = self.ptr(current + CLASS_FIELDS)?;
            let count = self.i32(current + CLASS_FIELD_COUNT)?;
            if array != 0 && (0..10_000).contains(&count) {
                for index in 0..count as u64 {
                    let field = array + index * FIELD_SIZE;
                    let attributes = self.u16(self.ptr(field)? + 8)?;
                    let Some(name) = self.cstr(self.ptr(field + 8)?)? else {
                        continue;
                    };
                    let offset = u64::try_from(self.i32(field + 0x18)?).unwrap_or(0);
                    fields
                        .entry(name)
                        .or_insert((offset, attributes & FIELD_ATTRIBUTE_STATIC != 0));
                }
            }
            current = self.ptr(current + CLASS_PARENT)?;
        }
        self.fields.borrow_mut().insert(class, fields.clone());
        Ok(fields)
    }

    fn field(&self, class: u64, name: &str) -> Result<(u64, bool)> {
        self.fields(class)?
            .get(name)
            .copied()
            .with_context(|| format!("field {name} not found"))
    }

    fn static_ref(&self, class: u64, name: &str) -> Result<u64> {
        let (offset, is_static) = self.field(class, name)?;
        ensure!(is_static, "field {name} is not static");
        let vtable = self.ptr(self.ptr(class + CLASS_RUNTIME_INFO)? + 8)?;
        let vtable_size = u64::try_from(self.i32(class + CLASS_VTABLE_SIZE)?)?;
        let statics = self.ptr(vtable + VTABLE_VTABLE + 8 * vtable_size)?;
        self.ptr(statics + offset)
    }

    /// Address of an instance field: object -> vtable -> class.
    fn field_address(&self, object: u64, name: &str) -> Result<u64> {
        ensure!(object != 0, "null object while reading {name}");
        let class = self.ptr(self.ptr(object)?)?;
        Ok(object + self.field(class, name)?.0)
    }

    fn get(&self, object: u64, name: &str) -> Result<u64> {
        self.ptr(self.field_address(object, name)?)
    }

    fn get_i32(&self, object: u64, name: &str) -> Result<i32> {
        self.i32(self.field_address(object, name)?)
    }

    fn get_u64(&self, object: u64, name: &str) -> Result<u64> {
        self.u64(self.field_address(object, name)?)
    }

    /// Items of a `List<T>` of references. Generic instances carry no field
    /// table, but the layout is fixed: `_items` at 0x10, `_size` at 0x18.
    fn list_items(&self, list: u64) -> Result<Vec<u64>> {
        ensure!(list != 0, "null list");
        let items = self.ptr(list + 0x10)?;
        let size = u64::try_from(self.i32(list + 0x18)?).context("bad list size")?;
        ensure!(size < 1 << 20, "bad list size {size}");
        (0..size)
            .map(|index| self.ptr(items + 0x20 + 8 * index))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_parent_pid_when_comm_has_spaces_and_parens() {
        let ppid = parent_pid(std::process::id());
        assert!(ppid.is_some_and(|ppid| ppid > 0));
        assert!(descends_from(std::process::id(), std::process::id()));
    }

    #[test]
    fn process_alive_tells_live_from_gone() {
        assert!(process_alive(std::process::id()));
        // Above the default pid_max, so no such process.
        assert!(!process_alive(u32::MAX));
    }

    #[test]
    fn dust_needs_exactly_one_agreed_value() {
        assert_eq!(single_dust(&HashSet::new()), None);
        assert_eq!(single_dust(&HashSet::from([2280])), Some(2280));
        assert_eq!(single_dust(&HashSet::from([2280, 2300])), None);
    }

    #[test]
    fn finds_an_exported_symbol_in_a_shared_library() {
        // libc is always mapped into the test binary.
        let maps = fs::read_to_string("/proc/self/maps").unwrap();
        let libc = maps
            .lines()
            .filter_map(|line| line.split_whitespace().nth(5))
            .find(|path| path.contains("libc.so"))
            .expect("libc mapped");
        let elf = fs::read(libc).unwrap();
        assert!(dynamic_symbol(&elf, "malloc").unwrap() > 0);
        assert!(dynamic_symbol(&elf, "no_such_symbol_here").is_err());
    }
}
