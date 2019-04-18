//! Representation of Cranelift IR functions.

mod builder;
pub mod condcodes;
pub mod dfg;
pub mod entities;
mod extfunc;
mod extname;
pub mod function;
mod globalvalue;
mod heap;
pub mod immediates;
pub mod instructions;
pub mod jumptable;
pub mod layout;
mod libcall;
mod memflags;
mod progpoint;
mod sourceloc;
pub mod stackslot;
mod table;
mod trapcode;
pub mod types;
mod valueloc;

pub use crate::ir::builder::{InsertBuilder, InstBuilder, InstBuilderBase, InstInserterBase};
pub use crate::ir::dfg::{DataFlowGraph, ValueDef};
pub use crate::ir::entities::{
    Ebb, FuncRef, GlobalValue, Heap, Inst, JumpTable, SigRef, StackSlot, Table, Value,
};
pub use crate::ir::extfunc::{
    AbiParam, ArgumentExtension, ArgumentPurpose, ExtFuncData, Signature,
};
pub use crate::ir::extname::ExternalName;
pub use crate::ir::function::Function;
pub use crate::ir::globalvalue::GlobalValueData;
pub use crate::ir::heap::{HeapData, HeapStyle};
pub use crate::ir::instructions::{
    InstructionData, Opcode, ValueList, ValueListPool, VariableArgs,
};
pub use crate::ir::jumptable::JumpTableData;
pub use crate::ir::layout::Layout;
pub use crate::ir::libcall::{get_libcall_funcref, get_probestack_funcref, LibCall};
pub use crate::ir::memflags::MemFlags;
pub use crate::ir::progpoint::{ExpandedProgramPoint, ProgramOrder, ProgramPoint};
pub use crate::ir::sourceloc::SourceLoc;
pub use crate::ir::stackslot::{StackSlotData, StackSlotKind, StackSlots};
pub use crate::ir::table::TableData;
pub use crate::ir::trapcode::TrapCode;
pub use crate::ir::types::Type;
pub use crate::ir::valueloc::{ArgumentLoc, ValueLoc};

use crate::binemit;
use crate::entity::{PrimaryMap, SecondaryMap};
use crate::isa;

/// Map of value locations.
pub type ValueLocations = SecondaryMap<Value, ValueLoc>;

/// Map of jump tables.
pub type JumpTables = PrimaryMap<JumpTable, JumpTableData>;

/// Map of instruction encodings.
pub type InstEncodings = SecondaryMap<Inst, isa::Encoding>;

/// Code offsets for EBBs.
pub type EbbOffsets = SecondaryMap<Ebb, binemit::CodeOffset>;

/// Code offsets for Jump Tables.
pub type JumpTableOffsets = SecondaryMap<JumpTable, binemit::CodeOffset>;

/// Source locations for instructions.
pub type SourceLocs = SecondaryMap<Inst, SourceLoc>;