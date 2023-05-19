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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    compute_cells: HashMap<ComputeCellId, ComputeCell<'a, T>>,
    input_cell_values: HashMap<InputCellId, T>,
    next_cell_id: u32,
}

type ComputeFun<'a, T> = Box<dyn 'a + Fn(&[T]) -> T>;

struct ComputeCell<'a, T> {
    value: T,
    compute_func: ComputeFun<'a, T>,
    dependencies: Vec<CellId>,
    callbacks: HashMap<CallbackId, Box<dyn 'a + FnMut(T)>>,
    next_callback_id: u32,
}

// You are guaranteed that Reactor will only be tested against types that are Copy + PartialEq.
impl<'a, T: Copy + PartialEq> Reactor<'a, T> {
    pub fn new() -> Self {
        Self {
            compute_matrix: HashMap::new(),
            compute_cells: HashMap::new(),
            input_cell_values: HashMap::new(),
            next_cell_id: 1,
        }
    }

    // Creates an input cell with the specified initial value, returning its ID.
    pub fn create_input(&mut self, initial: T) -> InputCellId {
        let cell_id = InputCellId(self.next_cell_id);
        self.next_cell_id += 1;
        self.input_cell_values.insert(cell_id, initial);
        cell_id
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
        dependencies
            .iter()
            .find(|&&cell_id| self.value(cell_id).is_none())
            .map_or(Ok(()), |cell_id| Err(*cell_id))?;

        let new_cell_id = ComputeCellId(self.next_cell_id);
        self.next_cell_id += 1;

        let compute_cell = ComputeCell {
            value: compute_func(&self.values(dependencies)),
            compute_func: Box::new(compute_func),
            callbacks: HashMap::new(),
            dependencies: Vec::from(dependencies),
            next_callback_id: u32::default(),
        };
        self.compute_cells.insert(new_cell_id, compute_cell);

        dependencies.iter().for_each(|dep_id| {
            self.compute_matrix
                .entry(*dep_id)
                .or_default()
                .push(new_cell_id);
        });

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
            CellId::Compute(compute_cell_id) => {
                self.compute_cells.get(&compute_cell_id).map(|c| c.value)
            }
            CellId::Input(input_cell_id) => self.input_cell_values.get(&input_cell_id).copied(),
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
        if !self.input_cell_values.contains_key(&input_cell_id) {
            return false;
        }

        self.input_cell_values
            .entry(input_cell_id)
            .and_modify(|value| *value = new_value);
        let mut to_recompute: VecDeque<ComputeCellId> = VecDeque::from(
            self.compute_matrix
                .entry(CellId::Input(input_cell_id))
                .or_default()
                .clone(),
        );

        let mut maybe_changed = HashMap::new();

        while !to_recompute.is_empty() {
            let compute_cell_id = to_recompute.pop_front().unwrap();
            let values = self.values(&self.compute_cells[&compute_cell_id].dependencies);
            let compute_cell = self.compute_cells.get_mut(&compute_cell_id).unwrap();
            maybe_changed
                .entry(compute_cell_id)
                .or_insert(compute_cell.value);
            compute_cell.value = (compute_cell.compute_func)(&values);

            self.compute_matrix
                .entry(CellId::Compute(compute_cell_id))
                .or_default()
                .iter()
                .for_each(|downstram| to_recompute.push_back(*downstram))
        }

        maybe_changed
            .into_iter()
            .for_each(|(compute_cell_id, old_value)| {
                let new_value = self.value(CellId::Compute(compute_cell_id)).unwrap();
                if old_value != new_value {
                    self.compute_cells
                        .get_mut(&compute_cell_id)
                        .unwrap()
                        .callbacks
                        .values_mut()
                        .for_each(|callback| (*callback)(new_value))
                }
            });

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
        self.compute_cells
            .get_mut(&compute_cell_id)
            .map(|compute_cell| {
                let callback_id = CallbackId(compute_cell.next_callback_id);
                compute_cell.next_callback_id += 1;
                compute_cell
                    .callbacks
                    .insert(callback_id, Box::new(callback));
                callback_id
            })
    }

    // Removes the specified callback, using an ID returned from add_callback.
    //
    // Returns an Err if either the cell or callback does not exist.
    //
    // A removed callback should no longer be called.
    pub fn remove_callback(
        &mut self,
        compute_cell_id: ComputeCellId,
        callback_id: CallbackId,
    ) -> Result<(), RemoveCallbackError> {
        let compute_cell = self
            .compute_cells
            .get_mut(&compute_cell_id)
            .ok_or(NonexistentCell)?;
        match compute_cell.callbacks.remove(&callback_id) {
            Some(_) => Ok(()),
            None => Err(NonexistentCallback),
        }
    }
}
