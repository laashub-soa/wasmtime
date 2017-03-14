//! A verifier for ensuring that functions are well formed.
//! It verifies:
//!
//!   EBB integrity
//!
//!    - All instructions reached from the `ebb_insts` iterator must belong to
//!      the EBB as reported by `inst_ebb()`.
//!    - Every EBB must end in a terminator instruction, and no other instruction
//!      can be a terminator.
//!    - Every value in the `ebb_args` iterator belongs to the EBB as reported by `value_ebb`.
//!
//!   Instruction integrity
//!
//!    - The instruction format must match the opcode.
//!    - All result values must be created for multi-valued instructions.
//!    - Instructions with no results must have a VOID `first_type()`.
//!    - All referenced entities must exist. (Values, EBBs, stack slots, ...)
//!
//!   SSA form
//!
//! TODO:
//!    - Values must be defined by an instruction that exists and that is inserted in
//!      an EBB, or be an argument of an existing EBB.
//!    - Values used by an instruction must dominate the instruction.
//!
//!   Control flow graph and dominator tree integrity:
//!
//!    - All predecessors in the CFG must be branches to the EBB.
//!    - All branches to an EBB must be present in the CFG.
//!    - A recomputed dominator tree is identical to the existing one.
//!
//!   Type checking
//!
//!    - Compare input and output values against the opcode's type constraints.
//!      For polymorphic opcodes, determine the controlling type variable first.
//!    - Branches and jumps must pass arguments to destination EBBs that match the
//!      expected types exactly. The number of arguments must match.
//!    - All EBBs in a jump_table must take no arguments.
//!    - Function calls are type checked against their signature.
//!    - The entry block must take arguments that match the signature of the current
//!      function.
//!    - All return instructions must have return value operands matching the current
//!      function signature.
//!
//!   Ad hoc checking
//!
//!    - Stack slot loads and stores must be in-bounds.
//!    - Immediate constraints for certain opcodes, like `udiv_imm v3, 0`.
//!    - Extend / truncate instructions have more type constraints: Source type can't be
//!      larger / smaller than result type.
//!    - `Insertlane` and `extractlane` instructions have immediate lane numbers that must be in
//!      range for their polymorphic type.
//!    - Swizzle and shuffle instructions take a variable number of lane arguments. The number
//!      of arguments must match the destination type, and the lane indexes must be in range.

use ir::{types, Function, ValueDef, Ebb, Inst, SigRef, FuncRef, ValueList, JumpTable, Value};
use ir::instructions::InstructionFormat;
use ir::entities::AnyEntity;
use std::fmt::{self, Display, Formatter};
use std::result;

/// A verifier error.
#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    /// The entity causing the verifier error.
    pub location: AnyEntity,
    /// Error message.
    pub message: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

/// Verifier result.
pub type Result<T> = result::Result<T, Error>;

// Create an `Err` variant of `Result<X>` from a location and `format!` arguments.
macro_rules! err {
    ( $loc:expr, $msg:expr ) => {
        Err(Error {
            location: $loc.into(),
            message: String::from($msg),
        })
    };

    ( $loc:expr, $fmt:expr, $( $arg:expr ),+ ) => {
        Err(Error {
            location: $loc.into(),
            message: format!( $fmt, $( $arg ),+ ),
        })
    };
}

/// Verify `func`.
pub fn verify_function(func: &Function) -> Result<()> {
    Verifier::new(func).run()
}

struct Verifier<'a> {
    func: &'a Function,
}

impl<'a> Verifier<'a> {
    pub fn new(func: &'a Function) -> Verifier {
        Verifier { func: func }
    }

    fn ebb_integrity(&self, ebb: Ebb, inst: Inst) -> Result<()> {

        let is_terminator = self.func.dfg[inst].opcode().is_terminator();
        let is_last_inst = self.func.layout.last_inst(ebb) == Some(inst);

        if is_terminator && !is_last_inst {
            // Terminating instructions only occur at the end of blocks.
            return err!(inst,
                        "a terminator instruction was encountered before the end of {}",
                        ebb);
        }
        if is_last_inst && !is_terminator {
            return err!(ebb, "block does not end in a terminator instruction!");
        }

        // Instructions belong to the correct ebb.
        let inst_ebb = self.func.layout.inst_ebb(inst);
        if inst_ebb != Some(ebb) {
            return err!(inst, "should belong to {} not {:?}", ebb, inst_ebb);
        }

        // Arguments belong to the correct ebb.
        for arg in self.func.dfg.ebb_args(ebb) {
            match self.func.dfg.value_def(arg) {
                ValueDef::Arg(arg_ebb, _) => {
                    if ebb != arg_ebb {
                        return err!(arg, "does not belong to {}", ebb);
                    }
                }
                _ => {
                    return err!(arg, "expected an argument, found a result");
                }
            }
        }

        Ok(())
    }

    fn instruction_integrity(&self, inst: Inst) -> Result<()> {
        let inst_data = &self.func.dfg[inst];
        let dfg = &self.func.dfg;

        // The instruction format matches the opcode
        if inst_data.opcode().format() != InstructionFormat::from(inst_data) {
            return err!(inst, "instruction opcode doesn't match instruction format");
        }

        let fixed_results = inst_data.opcode().constraints().fixed_results();
        // var_results is 0 if we aren't a call instruction
        let var_results =
            dfg.call_signature(inst).map(|sig| dfg.signatures[sig].return_types.len()).unwrap_or(0);
        let total_results = fixed_results + var_results;

        if total_results == 0 {
            // Instructions with no results have a NULL `first_type()`
            let ret_type = inst_data.first_type();
            if ret_type != types::VOID {
                return err!(inst,
                            "instruction expected to have NULL return value, found {}",
                            ret_type);
            }
        } else {
            // All result values for multi-valued instructions are created
            let got_results = dfg.inst_results(inst).count();
            if got_results != total_results {
                return err!(inst,
                            "expected {} result values, found {}",
                            total_results,
                            got_results);
            }
        }

        self.verify_entity_references(inst)
    }

    fn verify_entity_references(&self, inst: Inst) -> Result<()> {
        use ir::instructions::InstructionData::*;

        for &arg in self.func.dfg[inst].arguments(&self.func.dfg.value_lists) {
            self.verify_value(inst, arg)?;
        }

        for res in self.func.dfg.inst_results(inst) {
            self.verify_value(inst, res)?;
        }

        match &self.func.dfg[inst] {
            &MultiAry { ref args, .. } => {
                self.verify_value_list(inst, args)?;
            }
            &Jump { destination, ref args, .. } => {
                self.verify_ebb(inst, destination)?;
                self.verify_value_list(inst, args)?;
            }
            &Branch { destination, ref args, .. } => {
                self.verify_ebb(inst, destination)?;
                self.verify_value_list(inst, args)?;
            }
            &BranchTable { table, .. } => {
                self.verify_jump_table(inst, table)?;
            }
            &Call { func_ref, ref args, .. } => {
                self.verify_func_ref(inst, func_ref)?;
                self.verify_value_list(inst, args)?;
            }
            &IndirectCall { sig_ref, ref args, .. } => {
                self.verify_sig_ref(inst, sig_ref)?;
                self.verify_value_list(inst, args)?;
            }
            // Exhaustive list so we can't forget to add new formats
            &Nullary { .. } |
            &Unary { .. } |
            &UnaryImm { .. } |
            &UnaryIeee32 { .. } |
            &UnaryIeee64 { .. } |
            &UnarySplit { .. } |
            &Binary { .. } |
            &BinaryImm { .. } |
            &BinaryOverflow { .. } |
            &Ternary { .. } |
            &InsertLane { .. } |
            &ExtractLane { .. } |
            &IntCompare { .. } |
            &FloatCompare { .. } => {}
        }

        Ok(())
    }

    fn verify_ebb(&self, inst: Inst, e: Ebb) -> Result<()> {
        if !self.func.dfg.ebb_is_valid(e) {
            err!(inst, "invalid ebb reference {}", e)
        } else {
            Ok(())
        }
    }

    fn verify_sig_ref(&self, inst: Inst, s: SigRef) -> Result<()> {
        if !self.func
                .dfg
                .signatures
                .is_valid(s) {
            err!(inst, "invalid signature reference {}", s)
        } else {
            Ok(())
        }
    }

    fn verify_func_ref(&self, inst: Inst, f: FuncRef) -> Result<()> {
        if !self.func
                .dfg
                .ext_funcs
                .is_valid(f) {
            err!(inst, "invalid function reference {}", f)
        } else {
            Ok(())
        }
    }

    fn verify_value_list(&self, inst: Inst, l: &ValueList) -> Result<()> {
        if !l.is_valid(&self.func.dfg.value_lists) {
            err!(inst, "invalid value list reference {:?}", l)
        } else {
            Ok(())
        }
    }

    fn verify_jump_table(&self, inst: Inst, j: JumpTable) -> Result<()> {
        if !self.func.jump_tables.is_valid(j) {
            err!(inst, "invalid jump table reference {}", j)
        } else {
            Ok(())
        }
    }

    fn verify_value(&self, inst: Inst, v: Value) -> Result<()> {
        if !self.func.dfg.value_is_valid(v) {
            err!(inst, "invalid value reference {}", v)
        } else {
            Ok(())
        }
    }

    pub fn run(&self) -> Result<()> {
        for ebb in self.func.layout.ebbs() {
            for inst in self.func.layout.ebb_insts(ebb) {
                self.ebb_integrity(ebb, inst)?;
                self.instruction_integrity(inst)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Verifier, Error};
    use ir::Function;
    use ir::instructions::{InstructionData, Opcode};
    use ir::types;

    macro_rules! assert_err_with_msg {
        ($e:expr, $msg:expr) => (
            match $e {
                Ok(_) => { panic!("Expected an error!") },
                Err(Error { message, .. } ) => {
                    if !message.contains($msg) {
                       panic!(format!("'{}' did not contain the substring '{}'", message, $msg));
                    }
                }
            }
        )
    }

    #[test]
    fn empty() {
        let func = Function::new();
        let verifier = Verifier::new(&func);
        assert_eq!(verifier.run(), Ok(()));
    }

    #[test]
    fn bad_instruction_format() {
        let mut func = Function::new();
        let ebb0 = func.dfg.make_ebb();
        func.layout.append_ebb(ebb0);
        let nullary_with_bad_opcode = func.dfg.make_inst(InstructionData::Nullary {
                                                             opcode: Opcode::Jump,
                                                             ty: types::VOID,
                                                         });
        func.layout.append_inst(nullary_with_bad_opcode, ebb0);
        let verifier = Verifier::new(&func);
        assert_err_with_msg!(verifier.run(), "instruction format");
    }
}
