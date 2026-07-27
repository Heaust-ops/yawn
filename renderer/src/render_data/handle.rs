use bytemuck::{Pod, Zeroable};

macro_rules! handle {
    ($name:ident) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Pod, Zeroable)]
        pub struct $name {
            slot: u32,
            generation: u32,
        }

        impl $name {
            pub const fn from_parts(slot: u32, generation: u32) -> Self {
                Self { slot, generation }
            }

            pub const fn slot(self) -> u32 {
                self.slot
            }

            pub const fn generation(self) -> u32 {
                self.generation
            }
        }
    };
}

handle!(MeshHandle);
handle!(InstanceHandle);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotState {
    Occupied,
    Vacant { next: Option<u32> },
    Retired,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedSlot {
    pub slot: u32,
    pub generation: u32,
    reused_next: Option<u32>,
    append: bool,
}

pub(super) struct SlotTable {
    pub(super) generations: Vec<u32>,
    pub(super) states: Vec<SlotState>,
    free_head: Option<u32>,
    live_count: u32,
    logical_capacity: u32,
    pub(super) max_capacity: Option<u32>,
}

impl SlotTable {
    pub fn new(
        initial: u32,
        max: Option<u32>,
        resource: &'static str,
    ) -> Result<Self, crate::render_data::RenderDataError> {
        let mut table = Self {
            generations: Vec::new(),
            states: Vec::new(),
            free_head: None,
            live_count: 0,
            logical_capacity: 0,
            max_capacity: max,
        };
        table.reserve_for_len(initial, resource)?;
        Ok(table)
    }

    pub fn live_count(&self) -> u32 {
        self.live_count
    }

    pub fn logical_capacity(&self) -> u32 {
        self.logical_capacity
    }

    pub fn max_capacity(&self) -> Option<u32> {
        self.max_capacity
    }

    pub fn required_len_for_prepare(&self) -> Result<u32, crate::render_data::RenderDataError> {
        if self.free_head.is_some() {
            u32::try_from(self.generations.len()).map_err(|_| {
                crate::render_data::RenderDataError::CapacityOverflow { resource: "slots" }
            })
        } else {
            let len = u32::try_from(self.generations.len()).map_err(|_| {
                crate::render_data::RenderDataError::CapacityOverflow { resource: "slots" }
            })?;
            len.checked_add(1)
                .ok_or(crate::render_data::RenderDataError::CapacityOverflow { resource: "slots" })
        }
    }

    pub fn reserve_for_len(
        &mut self,
        required: u32,
        resource: &'static str,
    ) -> Result<(), crate::render_data::RenderDataError> {
        let target = crate::render_data::next_capacity(
            self.logical_capacity,
            required,
            self.max_capacity,
            resource,
        )?;
        crate::render_data::reserve_vec(&mut self.generations, target, resource)?;
        crate::render_data::reserve_vec(&mut self.states, target, resource)?;
        self.logical_capacity = target;
        Ok(())
    }

    pub fn prepare(&self) -> Result<PreparedSlot, crate::render_data::RenderDataError> {
        if let Some(slot) = self.free_head {
            let index = slot as usize;
            let SlotState::Vacant { next } = self.states[index] else {
                unreachable!("free list points to a non-vacant slot")
            };
            Ok(PreparedSlot {
                slot,
                generation: self.generations[index],
                reused_next: next,
                append: false,
            })
        } else {
            let slot = u32::try_from(self.generations.len()).map_err(|_| {
                crate::render_data::RenderDataError::CapacityOverflow { resource: "slots" }
            })?;
            Ok(PreparedSlot {
                slot,
                generation: 1,
                reused_next: None,
                append: true,
            })
        }
    }

    pub fn commit(&mut self, prepared: PreparedSlot) {
        if prepared.append {
            self.generations.push(prepared.generation);
            self.states.push(SlotState::Occupied);
        } else {
            self.free_head = prepared.reused_next;
            self.states[prepared.slot as usize] = SlotState::Occupied;
        }
        self.live_count += 1;
    }

    pub fn contains(&self, slot: u32, generation: u32) -> bool {
        let index = slot as usize;
        self.generations.get(index) == Some(&generation)
            && matches!(self.states.get(index), Some(SlotState::Occupied))
    }

    pub fn remove(&mut self, slot: u32, generation: u32) -> bool {
        if !self.contains(slot, generation) {
            return false;
        }
        let index = slot as usize;
        self.live_count -= 1;
        if generation == u32::MAX {
            self.states[index] = SlotState::Retired;
        } else {
            self.generations[index] = generation + 1;
            self.states[index] = SlotState::Vacant {
                next: self.free_head,
            };
            self.free_head = Some(slot);
        }
        true
    }

    pub fn clear(&mut self) {
        self.free_head = None;
        self.live_count = 0;
        for index in (0..self.states.len()).rev() {
            match self.states[index] {
                SlotState::Occupied if self.generations[index] == u32::MAX => {
                    self.states[index] = SlotState::Retired;
                }
                SlotState::Occupied => {
                    self.generations[index] += 1;
                    self.states[index] = SlotState::Vacant {
                        next: self.free_head,
                    };
                    self.free_head = Some(
                        u32::try_from(index).expect("slot table length was checked before append"),
                    );
                }
                SlotState::Vacant { .. } => {
                    self.states[index] = SlotState::Vacant {
                        next: self.free_head,
                    };
                    self.free_head = Some(
                        u32::try_from(index).expect("slot table length was checked before append"),
                    );
                }
                SlotState::Retired => {}
            }
        }
    }

    pub fn seed_successor(&mut self, predecessor: &Self) {
        self.generations.clear();
        self.states.clear();
        self.free_head = None;
        self.live_count = 0;
        for generation in predecessor.generations.iter().copied() {
            let generation = generation.saturating_add(1);
            self.generations.push(generation);
            if generation == u32::MAX {
                self.states.push(SlotState::Retired);
            } else {
                self.states.push(SlotState::Vacant {
                    next: self.free_head,
                });
                self.free_head = Some((self.states.len() - 1) as u32);
            }
        }
    }

    pub fn occupied(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.states.iter().enumerate().filter_map(|(index, state)| {
            matches!(state, SlotState::Occupied).then(|| {
                (
                    u32::try_from(index).expect("slot table length was checked before append"),
                    self.generations[index],
                )
            })
        })
    }

    #[cfg(test)]
    pub fn force_generation(&mut self, slot: u32, generation: u32) {
        self.generations[slot as usize] = generation;
    }
}
