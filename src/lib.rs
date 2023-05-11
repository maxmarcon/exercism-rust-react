use crate::RemoveCallbackError::{NonexistentCallback, NonexistentCell};

/// `InputCellId` is a unique identifier for an input cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputCellId(usize);
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputeCellId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallbackId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    // Just so that the compiler doesn't complain about an unused type parameter.
    // You probably want to delete this field.
    inputs: Vec<T>,
    compute_cells: Vec<ComputeCell<'a, T>>,
}

struct ComputeCell<'a, T> {
    dependencies: Vec<CellId>,
    compute_func: Box<dyn 'a + Fn(&[T]) -> T>,
    callbacks: Vec<Option<Box<dyn 'a + FnMut(T)>>>,
}

// You are guaranteed that Reactor will only be tested against types that are Copy + PartialEq.
impl<'a, T: Copy + PartialEq> Reactor<'a, T> {
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            compute_cells: Vec::new(),
        }
    }

    // Creates an input cell with the specified initial value, returning its ID.
    pub fn create_input(&mut self, initial: T) -> InputCellId {
        self.inputs.push(initial);
        InputCellId(self.inputs.len() - 1)
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
        let invalid_cell = dependencies.iter().find(|cell_id| match cell_id {
            CellId::Input(InputCellId(id)) => *id >= self.inputs.len(),
            CellId::Compute(ComputeCellId(id)) => *id >= self.compute_cells.len(),
        });

        match invalid_cell {
            Some(cell_id) => Err(*cell_id),
            None => {
                let compute_cell = ComputeCell {
                    dependencies: Vec::from(dependencies),
                    compute_func: Box::new(compute_func),
                    callbacks: Vec::new(),
                };
                self.compute_cells.push(compute_cell);
                Ok(ComputeCellId(self.compute_cells.len() - 1))
            }
        }
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
            CellId::Input(InputCellId(id)) => self.inputs.get(id).copied(),
            CellId::Compute(ComputeCellId(id)) => self
                .compute_cells
                .get(id)
                .map(|compute_cell| self.compute(compute_cell)),
        }
    }

    fn compute(&self, compute_cell: &ComputeCell<T>) -> T {
        let values: Vec<T> = compute_cell
            .dependencies
            .iter()
            .map(|&cell_id| self.value(cell_id).unwrap())
            .collect();

        (compute_cell.compute_func)(&values)
    }

    // Sets the value of the specified input cell.
    //
    // Returns false if the cell does not exist.
    //
    // Similarly, you may wonder about `get_mut(&mut self, id: CellId) -> Option<&mut Cell>`, with
    // a `set_value(&mut self, new_value: T)` method on `Cell`.
    //
    // As before, that turned out to add too much extra complexity.
    pub fn set_value(&mut self, InputCellId(id): InputCellId, new_value: T) -> bool {
        self.inputs.get_mut(id).map_or(false, |el| {
            *el = new_value;
            true
        })
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
        ComputeCellId(id): ComputeCellId,
        callback: F,
    ) -> Option<CallbackId> {
        self.compute_cells.get_mut(id).map_or(None, |compute_cell| {
            compute_cell.callbacks.push(Some(Box::new(callback)));
            Some(CallbackId(compute_cell.callbacks.len() - 1))
        })
    }

    // Removes the specified callback, using an ID returned from add_callback.
    //
    // Returns an Err if either the cell or callback does not exist.
    //
    // A removed callback should no longer be called.
    pub fn remove_callback(
        &mut self,
        ComputeCellId(cell_id): ComputeCellId,
        CallbackId(callback_id): CallbackId,
    ) -> Result<(), RemoveCallbackError> {
        self.compute_cells
            .get_mut(cell_id)
            .map_or(Err(NonexistentCell), |compute_cell| {
                compute_cell.callbacks.get_mut(callback_id).map_or(
                    Err(NonexistentCell),
                    |callback| match callback {
                        Some(_) => {
                            *callback = None;
                            Ok(())
                        }
                        None => Err(NonexistentCallback),
                    },
                )
            })
    }
}
