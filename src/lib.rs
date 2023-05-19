use crate::RemoveCallbackError::{NonexistentCallback, NonexistentCell};
use std::collections::{HashMap, VecDeque};

/// `InputCellId` is a unique identifier for an input cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InputCellId(u32);

/// `ComputeCellId` is a unique identifier for a compute cell.
/// Values of type `InputCellId` and `ComputeCellId` should not be mutually assignable,
/// demonstrated by the following tests:
///
/// ```compile_fail
/// let mut r = react::Reactor::new();
/// let input: react::ComputeCellId = r.create_input(111);
/// ```
///
/// ```compile_fail
/// let mut r = react::Reactor::new();
/// let input = r.create_input(111);
/// let compute: react::InputCellId = r.create_compute(&[react::CellId::Input(input)], |_| 222).unwrap();
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComputeCellId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallbackId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CellId {
    Input(InputCellId),
    Compute(ComputeCellId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RemoveCallbackError {
    NonexistentCell,
    NonexistentCallback,
}

pub struct Reactor<'a, T> {
    compute_matrix: HashMap<CellId, Vec<ComputeCellId>>,
    compute_cells: Vec<ComputeCell<'a, T>>,
    input_cells: Vec<T>,
}

struct ComputeCell<'a, T> {
    value: T,
    compute_func: Box<dyn 'a + Fn(&[T]) -> T>,
    dependencies: Vec<CellId>,
    callbacks: Vec<Option<Box<dyn 'a + FnMut(T)>>>,
}

// You are guaranteed that Reactor will only be tested against types that are Copy + PartialEq.
impl<'a, T: Copy + PartialEq> Reactor<'a, T> {
    pub fn new() -> Self {
        Self {
            compute_matrix: HashMap::default(),
            compute_cells: Vec::default(),
            input_cells: Vec::default(),
        }
    }

    fn valid(&self, cell_id: CellId) -> bool {
        match cell_id {
            CellId::Compute(ComputeCellId(id)) => id < self.compute_cells.len() as u32,
            CellId::Input(InputCellId(id)) => id < self.input_cells.len() as u32,
        }
    }

    // Creates an input cell with the specified initial value, returning its ID.
    pub fn create_input(&mut self, initial: T) -> InputCellId {
        self.input_cells.push(initial);
        InputCellId((self.input_cells.len() - 1) as u32)
    }

    // Creates a compute cell with the specified dependencies and compute function.
    // The compute function is expected to take in its arguments in the same order as specified in
    // `dependencies`.
    // You do not need to reject compute functions that expect more arguments than there are
    // dependencies (how would you check for this, anyway?).
    //
    // If any dependency doesn't exist, returns an Err with that nonexistent dependency.
    // (If multiple dependencies do not exist, exactly which one is returned is not defined and
    // will not be tested)
    //
    // Notice that there is no way to *remove* a cell.
    // This means that you may assume, without checking, that if the dependencies exist at creation
    // time they will continue to exist as long as the Reactor exists.
    pub fn create_compute<F: 'a + Fn(&[T]) -> T>(
        &mut self,
        dependencies: &[CellId],
        compute_func: F,
    ) -> Result<ComputeCellId, CellId> {
        if let Some(&cell_id) = dependencies.iter().find(|&&cell_id| !self.valid(cell_id)) {
            return Err(cell_id);
        }

        let compute_cell = ComputeCell {
            value: compute_func(&self.values(dependencies)),
            compute_func: Box::new(compute_func),
            callbacks: Vec::new(),
            dependencies: Vec::from(dependencies),
        };
        self.compute_cells.push(compute_cell);

        let new_cell_id = ComputeCellId((self.compute_cells.len() - 1) as u32);

        for &dep_id in dependencies {
            self.compute_matrix
                .entry(dep_id)
                .or_default()
                .push(new_cell_id);
        }

        Ok(new_cell_id)
    }

    // Retrieves the current value of the cell, or None if the cell does not exist.
    //
    // You may wonder whether it is possible to implement `get(&self, id: CellId) -> Option<&Cell>`
    // and have a `value(&self)` method on `Cell`.
    //
    // It turns out this introduces a significant amount of extra complexity to this exercise.
    // We chose not to cover this here, since this exercise is probably enough work as-is.
    pub fn value(&self, id: CellId) -> Option<T> {
        match id {
            CellId::Compute(ComputeCellId(id)) => {
                self.compute_cells.get(id as usize).map(|c| c.value)
            }
            CellId::Input(InputCellId(id)) => self.input_cells.get(id as usize).copied(),
        }
    }

    fn values(&self, cells: &[CellId]) -> Vec<T> {
        cells
            .iter()
            .map(|&cell_id| self.value(cell_id).unwrap())
            .collect()
    }

    // Sets the value of the specified input cell.
    //
    // Returns false if the cell does not exist.
    //
    // Similarly, you may wonder about `get_mut(&mut self, id: CellId) -> Option<&mut Cell>`, with
    // a `set_value(&mut self, new_value: T)` method on `Cell`.
    //
    // As before, that turned out to add too much extra complexity.
    pub fn set_value(&mut self, input_cell_id: InputCellId, new_value: T) -> bool {
        if !self.valid(CellId::Input(input_cell_id)) {
            return false;
        }

        self.input_cells[input_cell_id.0 as usize] = new_value;
        let mut to_recompute: VecDeque<ComputeCellId> = VecDeque::from(
            self.compute_matrix
                .entry(CellId::Input(input_cell_id))
                .or_default()
                .clone(),
        );

        let mut maybe_changed = HashMap::new();

        while !to_recompute.is_empty() {
            let compute_cell_id = to_recompute.pop_front().unwrap();
            let values = self.values(&self.compute_cells[compute_cell_id.0 as usize].dependencies);
            let compute_cell = &mut self.compute_cells[compute_cell_id.0 as usize];
            maybe_changed
                .entry(compute_cell_id)
                .or_insert(compute_cell.value);
            compute_cell.value = (compute_cell.compute_func)(&values);

            for downstream in self
                .compute_matrix
                .entry(CellId::Compute(compute_cell_id))
                .or_default()
            {
                to_recompute.push_back(*downstream);
            }
        }

        for (compute_cell_id, old_value) in maybe_changed {
            let new_value = self.value(CellId::Compute(compute_cell_id)).unwrap();
            if old_value != new_value {
                for callback in self.compute_cells[compute_cell_id.0 as usize]
                    .callbacks
                    .iter_mut()
                {
                    if let Some(callback) = callback {
                        (callback)(new_value);
                    }
                }
            }
        }

        true
    }

    // Adds a callback to the specified compute cell.
    //
    // Returns the ID of the just-added callback, or None if the cell doesn't exist.
    //
    // Callbacks on input cells will not be tested.
    //
    // The semantics of callbacks (as will be tested):
    // For a single set_value call, each compute cell's callbacks should each be called:
    // * Zero times if the compute cell's value did not change as a result of the set_value call.
    // * Exactly once if the compute cell's value changed as a result of the set_value call.
    //   The value passed to the callback should be the final value of the compute cell after the
    //   set_value call.
    pub fn add_callback<F: 'a + FnMut(T)>(
        &mut self,
        compute_cell_id: ComputeCellId,
        callback: F,
    ) -> Option<CallbackId> {
        if self.valid(CellId::Compute(compute_cell_id)) {
            let compute_cell = &mut self.compute_cells[compute_cell_id.0 as usize];
            compute_cell.callbacks.push(Some(Box::new(callback)));
            Some(CallbackId((compute_cell.callbacks.len() - 1) as u32))
        } else {
            None
        }
    }

    // Removes the specified callback, using an ID returned from add_callback.
    //
    // Returns an Err if either the cell or callback does not exist.
    //
    // A removed callback should no longer be called.
    pub fn remove_callback(
        &mut self,
        cell_id: ComputeCellId,
        CallbackId(callback_id): CallbackId,
    ) -> Result<(), RemoveCallbackError> {
        if !self.valid(CellId::Compute(cell_id)) {
            return Err(NonexistentCell);
        }

        let compute_cell = &mut self.compute_cells[cell_id.0 as usize];
        let callback = compute_cell
            .callbacks
            .get_mut(callback_id as usize)
            .ok_or(NonexistentCallback)?;

        if callback.is_some() {
            *callback = None;
            Ok(())
        } else {
            Err(NonexistentCallback)
        }
    }
}
