use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

const ALIGNMENT: u32 = 64;

#[derive(Clone, Serialize)]
pub struct Rows {
    pub name: String,
    pub rows: u32,
    pub stride: u32,
    pub format: String,
    pub offset: u32,
    pub bytes: u32,
}

#[derive(Default)]
struct Slots {
    active: HashSet<u32>,
    free: Vec<u32>,
    next: u32,
}

pub struct RenderData {
    arena: Box<[u8]>,
    arena_start: usize,
    base: u32,
    free: BTreeMap<u32, u32>,
    rows: HashMap<String, Rows>,
    slots: HashMap<String, Slots>,
}

impl RenderData {
    pub fn new(capacity: u32) -> Result<Self, &'static str> {
        if capacity < ALIGNMENT || capacity > u32::MAX - (ALIGNMENT - 1) {
            return Err("INIT");
        }
        let mut arena = vec![0; (capacity + ALIGNMENT - 1) as usize].into_boxed_slice();
        let pointer = arena.as_mut_ptr() as usize;
        let aligned = (pointer + ALIGNMENT as usize - 1) & !(ALIGNMENT as usize - 1);
        let mut data = Self {
            arena,
            arena_start: aligned - pointer,
            base: aligned as u32,
            free: BTreeMap::from([(0, capacity)]),
            rows: HashMap::new(),
            slots: HashMap::new(),
        };
        data.create_rows("info".into(), 1, 32, "f32".into())?;
        Ok(data)
    }

    pub fn create_rows(
        &mut self,
        name: String,
        rows: u32,
        stride: u32,
        format: String,
    ) -> Result<Rows, &'static str> {
        if name.is_empty()
            || rows == 0
            || stride < 16
            || stride % 16 != 0
            || !matches!(format.as_str(), "f32" | "u32" | "i32")
        {
            return Err("ROWS");
        }
        if let Some(current) = self.rows.get(&name).cloned() {
            if current.stride != stride || current.format != format {
                return Err("ROWS");
            }
            if rows <= current.rows {
                return Ok(current);
            }
            if name == "info" {
                return Err("ROWS_BUILTIN");
            }
            return self.grow_rows(current, rows);
        }
        let bytes = rows.checked_mul(stride).ok_or("ARENA_OOM")?;
        let offset = self.reserve(bytes)?;
        let end = offset + bytes;
        self.arena[self.arena_start + offset as usize..self.arena_start + end as usize].fill(0);
        let descriptor = Rows {
            name: name.clone(),
            rows,
            stride,
            format,
            offset: self.base + offset,
            bytes,
        };
        self.rows.insert(name, descriptor.clone());
        self.slots.insert(descriptor.name.clone(), Slots::default());
        Ok(descriptor)
    }

    pub fn delete_rows(&mut self, name: &str) -> Result<(), &'static str> {
        if name == "info" {
            return Err("ROWS_BUILTIN");
        }
        if self
            .slots
            .get(name)
            .is_some_and(|slots| !slots.active.is_empty())
        {
            return Err("ROWS_ACTIVE");
        }
        let rows = self.rows.remove(name).ok_or("ROWS_UNKNOWN")?;
        self.slots.remove(name);
        self.release(rows.offset - self.base, rows.bytes);
        Ok(())
    }

    pub fn rows(&self, name: &str) -> Option<&Rows> {
        self.rows.get(name)
    }

    pub fn descriptors(&self) -> Vec<Rows> {
        let mut rows = self.rows.values().cloned().collect::<Vec<_>>();
        rows.sort_by_key(|rows| rows.offset);
        rows
    }

    pub fn bytes(&self, name: &str) -> Option<&[u8]> {
        let rows = self.rows(name)?;
        let start = self.arena_start + (rows.offset - self.base) as usize;
        Some(&self.arena[start..start + rows.bytes as usize])
    }

    pub fn allocate_object(&mut self, name: &str) -> Result<(u32, bool), &'static str> {
        if name == "info" {
            return Err("ROWS_BUILTIN");
        }
        let slots = self.slots.get(name).ok_or("ROWS_UNKNOWN")?;
        let reused = slots.free.last().copied();
        let id = reused.unwrap_or(slots.next);
        let next = id.checked_add(1).ok_or("OBJECT_LIMIT")?;
        let capacity = self.rows[name].rows;
        let grew = id >= capacity;
        if grew {
            let descriptor = self.rows[name].clone();
            self.grow_rows(descriptor, next)?;
        }
        let slots = self.slots.get_mut(name).unwrap();
        if reused.is_some() {
            slots.free.pop();
        } else {
            slots.next = next;
        }
        slots.active.insert(id);
        Ok((id, grew))
    }

    pub fn delete_object(&mut self, name: &str, id: u32) -> Result<(), &'static str> {
        let slots = self.slots.get_mut(name).ok_or("ROWS_UNKNOWN")?;
        if !slots.active.remove(&id) {
            return Err("OBJECT_UNKNOWN");
        }
        let rows = self.rows[name].clone();
        let start = self.arena_start + (rows.offset - self.base + id * rows.stride) as usize;
        self.arena[start..start + rows.stride as usize].fill(0);
        slots.free.push(id);
        Ok(())
    }

    fn grow_rows(&mut self, mut descriptor: Rows, rows: u32) -> Result<Rows, &'static str> {
        let bytes = rows.checked_mul(descriptor.stride).ok_or("ARENA_OOM")?;
        let old_offset = descriptor.offset - self.base;
        let old_end = old_offset + descriptor.bytes;
        let extra = bytes - descriptor.bytes;
        if self
            .free
            .get(&old_end)
            .is_some_and(|available| *available >= extra)
        {
            let available = self.free.remove(&old_end).unwrap();
            if available > extra {
                self.free.insert(old_end + extra, available - extra);
            }
            self.arena[self.arena_start + old_end as usize
                ..self.arena_start + (old_end + extra) as usize]
                .fill(0);
        } else {
            let offset = self.reserve(bytes)?;
            let start = self.arena_start + offset as usize;
            self.arena[start..start + bytes as usize].fill(0);
            self.arena.copy_within(
                self.arena_start + old_offset as usize..self.arena_start + old_end as usize,
                start,
            );
            self.release(old_offset, descriptor.bytes);
            descriptor.offset = self.base + offset;
        }
        descriptor.rows = rows;
        descriptor.bytes = bytes;
        self.rows
            .insert(descriptor.name.clone(), descriptor.clone());
        Ok(descriptor)
    }

    pub fn update_info(&mut self, delta: f32, frame: u32, elapsed: f32, fps: u32) {
        let start = self.arena_start + (self.rows["info"].offset - self.base) as usize;
        for (index, value) in [delta, frame as f32, elapsed, fps as f32]
            .iter()
            .enumerate()
        {
            self.arena[start + index * 4..start + index * 4 + 4]
                .copy_from_slice(&value.to_le_bytes());
        }
    }

    pub fn skip_render(&self) -> bool {
        let rows = &self.rows["info"];
        let start = self.arena_start + (rows.offset - self.base) as usize + 16;
        f32::from_le_bytes(self.arena[start..start + 4].try_into().unwrap()) == 1.0
    }

    fn reserve(&mut self, bytes: u32) -> Result<u32, &'static str> {
        let (block, block_bytes, offset) = self
            .free
            .iter()
            .find_map(|(&block, &block_bytes)| {
                let offset = align(block)?;
                (offset.checked_add(bytes)? <= block.checked_add(block_bytes)?).then_some((
                    block,
                    block_bytes,
                    offset,
                ))
            })
            .ok_or("ARENA_OOM")?;
        self.free.remove(&block);
        if offset > block {
            self.free.insert(block, offset - block);
        }
        let end = offset + bytes;
        let block_end = block + block_bytes;
        if end < block_end {
            self.free.insert(end, block_end - end);
        }
        Ok(offset)
    }

    fn release(&mut self, mut offset: u32, mut bytes: u32) {
        if let Some((&previous, &previous_bytes)) = self.free.range(..offset).next_back() {
            if previous + previous_bytes == offset {
                self.free.remove(&previous);
                offset = previous;
                bytes += previous_bytes;
            }
        }
        if let Some((&next, &next_bytes)) = self.free.range(offset..).next() {
            if offset + bytes == next {
                self.free.remove(&next);
                bytes += next_bytes;
            }
        }
        self.free.insert(offset, bytes);
    }
}

fn align(value: u32) -> Option<u32> {
    value
        .checked_add(ALIGNMENT - 1)
        .map(|value| value & !(ALIGNMENT - 1))
}
