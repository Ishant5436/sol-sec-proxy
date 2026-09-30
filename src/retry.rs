//! Table-driven mapping from an error variant to what an autonomous agent
//! should do next. Each row has a one-line reason above it; the reasoning is
//! the contract, so change the comment together with the class.

use crate::tx_error::{ErrorSource, INSTRUCTION_ERROR_VARIANTS, TRANSACTION_ERROR_VARIANTS};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryClass {
    /// Re-sign with a recent blockhash and resend.
    RebuildWithFreshBlockhash,
    /// The transaction already landed; resending would be a duplicate.
    AlreadyLanded,
    /// The payer or an account needs more lamports before any retry can work.
    NeedsFunds,
    /// Simulate again and request a larger compute unit limit.
    RaiseComputeLimit,
    /// Deterministic failure caused by the transaction contents; change them.
    FixInputs,
    /// Transient cluster state (congestion, block cost caps, locks); try again later.
    RetryLater,
    /// Deterministic failure that changing inputs is unlikely to fix.
    Fatal,
    /// The proxy could not classify the error.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub class: RetryClass,
    pub action: &'static str,
}

use RetryClass::{
    AlreadyLanded, Fatal, FixInputs, NeedsFunds, RaiseComputeLimit, RebuildWithFreshBlockhash,
    RetryLater, Unknown,
};

const UNKNOWN_ACTION: &str =
    "Unrecognised error; inspect the raw simulation error and logs before retrying.";

/// (variant, class, suggested action) for `TransactionError` variants.
/// `InstructionError` is intentionally absent: it is unwrapped into the
/// instruction table below.
pub const TRANSACTION_TABLE: &[(&str, RetryClass, &str)] = &[
    // Another in-flight transaction holds a write lock; locks clear within a slot or two.
    (
        "AccountInUse",
        RetryLater,
        "An account is locked by another transaction; retry in a moment.",
    ),
    // The same pubkey appears twice in the account list, which is a construction bug.
    (
        "AccountLoadedTwice",
        FixInputs,
        "Remove the duplicate account key from the message.",
    ),
    // Fee payer account does not exist on chain, so it has no lamports to pay the fee.
    (
        "AccountNotFound",
        NeedsFunds,
        "Fund the fee payer account, then resend.",
    ),
    // The invoked program is not deployed at that address; retrying cannot create it.
    (
        "ProgramAccountNotFound",
        FixInputs,
        "Check the program id and the cluster; the program is not deployed.",
    ),
    // Fee payer cannot cover the base and priority fee.
    (
        "InsufficientFundsForFee",
        NeedsFunds,
        "Add SOL to the fee payer or lower the priority fee.",
    ),
    // Fee payer exists but is not a valid fee payer (for example it holds data or is not system-owned).
    (
        "InvalidAccountForFee",
        FixInputs,
        "Use a system-owned account without data as fee payer.",
    ),
    // Same signature was already accepted, so the transfer likely happened.
    (
        "AlreadyProcessed",
        AlreadyLanded,
        "Do not resend; look up the signature to confirm the earlier result.",
    ),
    // The blockhash expired or is unknown to this node; only a new blockhash fixes it.
    (
        "BlockhashNotFound",
        RebuildWithFreshBlockhash,
        "Fetch a fresh blockhash, re-sign, and resend.",
    ),
    // Call depth is a property of the program logic, not of cluster state.
    (
        "CallChainTooDeep",
        FixInputs,
        "Reduce nested cross-program invocations.",
    ),
    // A required fee-payer signature is missing from the transaction.
    (
        "MissingSignatureForFee",
        FixInputs,
        "Sign the transaction with the fee payer key.",
    ),
    // An instruction references an account index outside the account list.
    (
        "InvalidAccountIndex",
        FixInputs,
        "Rebuild the message; an account index is out of range.",
    ),
    // A signature does not verify against the message; re-signing the same message will not help.
    (
        "SignatureFailure",
        FixInputs,
        "Re-sign with the correct keys after rebuilding the message.",
    ),
    // A non-executable or unsupported account was used as a program.
    (
        "InvalidProgramForExecution",
        FixInputs,
        "Point the instruction at an executable program account.",
    ),
    // The message failed structural validation (bad header, indexes, or lengths).
    (
        "SanitizeFailure",
        FixInputs,
        "Rebuild the transaction; the message is malformed.",
    ),
    // The cluster is under maintenance; it is temporary and outside the caller's control.
    (
        "ClusterMaintenance",
        RetryLater,
        "The cluster is in maintenance; retry later.",
    ),
    // Runtime bookkeeping fault, not attributable to caller input and not fixed by resending.
    (
        "AccountBorrowOutstanding",
        Fatal,
        "Runtime borrow error; simplify account usage or report it.",
    ),
    // The block is full of compute; a later block has room.
    (
        "WouldExceedMaxBlockCostLimit",
        RetryLater,
        "The block cost limit was hit; retry, optionally with a higher priority fee.",
    ),
    // The node does not support this transaction version.
    (
        "UnsupportedVersion",
        FixInputs,
        "Use a legacy transaction or a node that supports this version.",
    ),
    // A readonly or otherwise non-writable account was marked writable.
    (
        "InvalidWritableAccount",
        FixInputs,
        "Mark the account readonly or use a writable account.",
    ),
    // A single account hit its per-block write cost cap; hot accounts clear next block.
    (
        "WouldExceedMaxAccountCostLimit",
        RetryLater,
        "A hot account hit its block write limit; retry later.",
    ),
    // Per-block account data growth cap was hit; resets next block.
    (
        "WouldExceedAccountDataBlockLimit",
        RetryLater,
        "The block account data limit was hit; retry later.",
    ),
    // More account locks than the cluster allows in one transaction.
    (
        "TooManyAccountLocks",
        FixInputs,
        "Split the transaction to lock fewer accounts.",
    ),
    // The lookup table address does not exist or is not visible yet.
    (
        "AddressLookupTableNotFound",
        FixInputs,
        "Check the address lookup table address and that it is activated.",
    ),
    // The lookup table account is not owned by the lookup table program.
    (
        "InvalidAddressLookupTableOwner",
        FixInputs,
        "Use a real address lookup table account.",
    ),
    // The lookup table account data cannot be decoded.
    (
        "InvalidAddressLookupTableData",
        FixInputs,
        "Use a valid, initialised address lookup table.",
    ),
    // The lookup index is past the end of the table.
    (
        "InvalidAddressLookupTableIndex",
        FixInputs,
        "Use an index inside the lookup table's address list.",
    ),
    // A write would leave an account below the rent exempt minimum.
    (
        "InvalidRentPayingAccount",
        NeedsFunds,
        "Top up the account to the rent exempt minimum or close it fully.",
    ),
    // Vote transactions have their own block cost budget that refills next block.
    (
        "WouldExceedMaxVoteCostLimit",
        RetryLater,
        "The block vote cost limit was hit; retry later.",
    ),
    // A global account data cap; it does not free up by resending the same transaction.
    (
        "WouldExceedAccountDataTotalLimit",
        Fatal,
        "The cluster-wide account data limit was reached; allocate less data.",
    ),
    // The same instruction appears twice where duplicates are disallowed.
    (
        "DuplicateInstruction",
        FixInputs,
        "Remove the duplicated instruction.",
    ),
    // An account would end below rent exemption after the transaction.
    (
        "InsufficientFundsForRent",
        NeedsFunds,
        "Add lamports so the account stays rent exempt, then resend.",
    ),
    // The loaded accounts data size cap is enforced per transaction, so fewer or smaller accounts fix it.
    (
        "MaxLoadedAccountsDataSizeExceeded",
        FixInputs,
        "Load fewer or smaller accounts, or set a higher loaded accounts data size limit.",
    ),
    // The requested loaded accounts data size limit is out of the allowed range.
    (
        "InvalidLoadedAccountsDataSizeLimit",
        FixInputs,
        "Set the loaded accounts data size limit within the allowed range.",
    ),
    // The validator asks the client to resubmit so the transaction can be re-sanitised.
    (
        "ResanitizationNeeded",
        RetryLater,
        "The node asked for a resubmit; retry.",
    ),
    // Program execution is briefly restricted, for example while being redeployed.
    (
        "ProgramExecutionTemporarilyRestricted",
        RetryLater,
        "The program is temporarily restricted; retry after a few slots.",
    ),
    // The transaction changes total lamports, which means a malformed instruction set.
    (
        "UnbalancedTransaction",
        FixInputs,
        "Fix the instructions so lamports balance.",
    ),
    // The program cache is full; it turns over as slots advance.
    (
        "ProgramCacheHitMaxLimit",
        RetryLater,
        "The program cache is full; retry later.",
    ),
    // Commit was cancelled by the node (for example around a slot boundary); resubmit works.
    (
        "CommitCancelled",
        RetryLater,
        "The commit was cancelled by the node; retry.",
    ),
    // Internal sentinel with no documented user-facing meaning.
    ("BailOut", Unknown, UNKNOWN_ACTION),
];

/// (variant, class, suggested action) for `InstructionError` variants.
pub const INSTRUCTION_TABLE: &[(&str, RetryClass, &str)] = &[
    // Generic failure with no detail; only logs can tell more.
    ("GenericError", Unknown, UNKNOWN_ACTION),
    // The program rejected an argument, which is deterministic for the same inputs.
    (
        "InvalidArgument",
        FixInputs,
        "Check the instruction arguments.",
    ),
    // The instruction data did not deserialize.
    (
        "InvalidInstructionData",
        FixInputs,
        "Rebuild the instruction data for the target program.",
    ),
    // An account's data is not in the format the program expects.
    (
        "InvalidAccountData",
        FixInputs,
        "Check that each account is the right type and initialised.",
    ),
    // Account data is too small for the operation, for example a missing realloc.
    (
        "AccountDataTooSmall",
        FixInputs,
        "Allocate a larger account or realloc it first.",
    ),
    // The instruction needs more lamports than the source account holds.
    (
        "InsufficientFunds",
        NeedsFunds,
        "Fund the source account, then resend.",
    ),
    // An account is owned by a different program than the instruction expects.
    (
        "IncorrectProgramId",
        FixInputs,
        "Pass accounts owned by the expected program.",
    ),
    // A signer the program requires did not sign.
    (
        "MissingRequiredSignature",
        FixInputs,
        "Add the missing signer and re-sign.",
    ),
    // The account is already initialised, so init again will always fail.
    (
        "AccountAlreadyInitialized",
        FixInputs,
        "Skip initialisation or use a fresh account.",
    ),
    // The account must be initialised before use.
    (
        "UninitializedAccount",
        FixInputs,
        "Initialise the account first.",
    ),
    // Lamport totals across the instruction accounts do not balance.
    (
        "UnbalancedInstruction",
        FixInputs,
        "Fix the instruction so lamports balance.",
    ),
    // A program changed an account's owner program id illegally.
    (
        "ModifiedProgramId",
        Fatal,
        "Program bug: it changed a program id it does not own.",
    ),
    // A program spent lamports from an account it does not own.
    (
        "ExternalAccountLamportSpend",
        Fatal,
        "Program bug: it debited an account it does not own.",
    ),
    // A program modified data of an account it does not own.
    (
        "ExternalAccountDataModified",
        Fatal,
        "Program bug: it modified an account it does not own.",
    ),
    // A program changed lamports on a readonly account.
    (
        "ReadonlyLamportChange",
        FixInputs,
        "Mark the account writable in the instruction.",
    ),
    // A program changed data on a readonly account.
    (
        "ReadonlyDataModified",
        FixInputs,
        "Mark the account writable in the instruction.",
    ),
    // Deprecated variant; duplicates are now allowed, so its meaning is unclear.
    ("DuplicateAccountIndex", Unknown, UNKNOWN_ACTION),
    // A program modified an executable account.
    (
        "ExecutableModified",
        Fatal,
        "Program bug: it modified an executable account.",
    ),
    // A program changed rent epoch, which the runtime forbids.
    (
        "RentEpochModified",
        Fatal,
        "Program bug: it changed the rent epoch.",
    ),
    // The instruction was given fewer account keys than the program needs.
    (
        "NotEnoughAccountKeys",
        FixInputs,
        "Pass all accounts the instruction requires.",
    ),
    // A program resized an account it may not resize.
    (
        "AccountDataSizeChanged",
        Fatal,
        "Program bug: it resized an account illegally.",
    ),
    // A non-executable account was invoked as a program.
    (
        "AccountNotExecutable",
        FixInputs,
        "Invoke an executable program account.",
    ),
    // The account is already borrowed by another part of the same instruction.
    (
        "AccountBorrowFailed",
        FixInputs,
        "Avoid passing the same account twice in conflicting roles.",
    ),
    // The account was still borrowed when the instruction ended.
    (
        "AccountBorrowOutstanding",
        Fatal,
        "Program bug: an account borrow outlived the instruction.",
    ),
    // Duplicate account entries got out of sync inside the runtime.
    (
        "DuplicateAccountOutOfSync",
        Fatal,
        "Runtime consistency error; report it with the transaction.",
    ),
    // The program's own error; the program name and code decide the fix, and it is deterministic.
    (
        "Custom",
        FixInputs,
        "Read the program error name and message, then correct the instruction inputs.",
    ),
    // Runtime-internal sentinel with no user-facing meaning.
    ("InvalidError", Unknown, UNKNOWN_ACTION),
    // Executable account data changed, which the runtime forbids.
    (
        "ExecutableDataModified",
        Fatal,
        "Program bug: it modified executable data.",
    ),
    // Executable account lamports changed, which the runtime forbids.
    (
        "ExecutableLamportChange",
        Fatal,
        "Program bug: it changed executable lamports.",
    ),
    // An executable account is not rent exempt.
    (
        "ExecutableAccountNotRentExempt",
        NeedsFunds,
        "Fund the program account to rent exemption.",
    ),
    // The invoked program id is not supported by the runtime.
    (
        "UnsupportedProgramId",
        FixInputs,
        "Use a supported program id.",
    ),
    // Cross-program invocation depth limit reached; it comes from program structure.
    ("CallDepth", Fatal, "Reduce nested program invocations."),
    // A CPI referenced an account missing from the instruction.
    (
        "MissingAccount",
        FixInputs,
        "Pass every account the program invokes.",
    ),
    // A program re-entered itself, which is not allowed.
    (
        "ReentrancyNotAllowed",
        Fatal,
        "Program design error: it re-enters itself.",
    ),
    // A PDA seed is longer than the maximum.
    ("MaxSeedLengthExceeded", FixInputs, "Shorten the PDA seeds."),
    // The seeds do not derive a valid address.
    ("InvalidSeeds", FixInputs, "Check the PDA seeds and bump."),
    // A realloc request was invalid.
    (
        "InvalidRealloc",
        FixInputs,
        "Request a valid new account size.",
    ),
    // The instruction ran out of compute units; more budget is the fix.
    (
        "ComputationalBudgetExceeded",
        RaiseComputeLimit,
        "Simulate, then set a higher compute unit limit with some headroom.",
    ),
    // A CPI tried to elevate signer or writable privileges.
    (
        "PrivilegeEscalation",
        FixInputs,
        "Pass accounts with the privileges the callee needs.",
    ),
    // The VM could not be set up for this program.
    (
        "ProgramEnvironmentSetupFailure",
        Fatal,
        "Runtime could not set up the program; report it.",
    ),
    // The program crashed or aborted; a program bug for these inputs.
    (
        "ProgramFailedToComplete",
        Fatal,
        "The program aborted; inspect the logs for a panic.",
    ),
    // The program failed to compile for the VM.
    (
        "ProgramFailedToCompile",
        Fatal,
        "The deployed program is not valid; redeploy it.",
    ),
    // The account is immutable, so no change to the transaction can modify it.
    (
        "Immutable",
        FixInputs,
        "Do not try to modify an immutable account.",
    ),
    // The signer is not the authority the program expects.
    (
        "IncorrectAuthority",
        FixInputs,
        "Sign with the correct authority.",
    ),
    // A Borsh encode or decode failed inside the program.
    (
        "BorshIoError",
        FixInputs,
        "Check that instruction and account data match the program's schema.",
    ),
    // The account balance is under rent exemption.
    (
        "AccountNotRentExempt",
        NeedsFunds,
        "Top up the account to the rent exempt minimum.",
    ),
    // The account owner is not the expected owner.
    (
        "InvalidAccountOwner",
        FixInputs,
        "Pass an account owned by the expected program.",
    ),
    // An arithmetic operation overflowed, which depends on the inputs.
    (
        "ArithmeticOverflow",
        FixInputs,
        "Use smaller amounts or fix the calculation inputs.",
    ),
    // The sysvar is not supported here.
    (
        "UnsupportedSysvar",
        FixInputs,
        "Use a supported sysvar account.",
    ),
    // The account owner is illegal for this operation.
    (
        "IllegalOwner",
        FixInputs,
        "Assign an allowed owner program.",
    ),
    // Allocation budget across the transaction is used up; deterministic for these inputs.
    (
        "MaxAccountsDataAllocationsExceeded",
        FixInputs,
        "Allocate less account data in this transaction.",
    ),
    // Too many accounts in one instruction.
    (
        "MaxAccountsExceeded",
        FixInputs,
        "Pass fewer accounts to the instruction.",
    ),
    // Too many nested instructions were recorded in the trace.
    (
        "MaxInstructionTraceLengthExceeded",
        FixInputs,
        "Split the work into more than one transaction.",
    ),
    // A builtin program must charge compute units; it was invoked with none available.
    (
        "BuiltinProgramsMustConsumeComputeUnits",
        RaiseComputeLimit,
        "Raise the compute unit limit.",
    ),
    // Internal sentinel with no documented user-facing meaning.
    ("BailOut", Unknown, UNKNOWN_ACTION),
];

fn lookup(table: &[(&str, RetryClass, &'static str)], kind: &str) -> Option<RetryPolicy> {
    table
        .iter()
        .find(|(name, _, _)| *name == kind)
        .map(|(_, class, action)| RetryPolicy {
            class: *class,
            action,
        })
}

/// Classifies `kind` for the given source. Unrecognised kinds map to `Unknown`.
pub fn classify(source: ErrorSource, kind: &str) -> RetryPolicy {
    let primary = match source {
        ErrorSource::Transaction => TRANSACTION_TABLE,
        ErrorSource::Instruction => INSTRUCTION_TABLE,
    };
    lookup(primary, kind).unwrap_or(RetryPolicy {
        class: Unknown,
        action: UNKNOWN_ACTION,
    })
}

/// Names present in the variant lists but missing from a table. Used by tests
/// to keep the tables exhaustive when the SDK lists change.
pub fn uncovered_variants() -> Vec<String> {
    let mut missing = Vec::new();
    for name in TRANSACTION_ERROR_VARIANTS {
        // InstructionError is unwrapped, never classified on its own.
        if *name != "InstructionError" && lookup(TRANSACTION_TABLE, name).is_none() {
            missing.push(format!("transaction:{}", name));
        }
    }
    for name in INSTRUCTION_ERROR_VARIANTS {
        if lookup(INSTRUCTION_TABLE, name).is_none() {
            missing.push(format!("instruction:{}", name));
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tables_cover_every_variant() {
        assert!(uncovered_variants().is_empty());
    }

    #[test]
    fn test_table_has_no_duplicate_rows() {
        for table in [TRANSACTION_TABLE, INSTRUCTION_TABLE] {
            for (i, (name, _, _)) in table.iter().enumerate() {
                assert!(table[i + 1..].iter().all(|(other, _, _)| other != name));
            }
        }
    }

    #[test]
    fn test_spot_classes() {
        let p = classify(ErrorSource::Transaction, "BlockhashNotFound");
        assert_eq!(p.class, RebuildWithFreshBlockhash);
        let p = classify(ErrorSource::Instruction, "Custom");
        assert_eq!(p.class, FixInputs);
        let p = classify(ErrorSource::Instruction, "Nope");
        assert_eq!(p.class, Unknown);
    }
}
