////////////////////////////////////////////////////
////////////////// AUTO-GENERATED //////////////////
////////////////////////////////////////////////////

#![no_std]

elrond_wasm_node::wasm_endpoints! {
    farm
    (
        init
        callBack
        acceptFee
        calculateRewardsForGivenPosition
        claimRewards
        compoundRewards
        end_produce_rewards
        enterFarm
        enterFarmAndLockRewards
        exitFarm
        getBurnedTokenAmount
        getCurrentBlockFee
        getDivisionSafetyConstant
        getFarmMigrationConfiguration
        getFarmTokenId
        getFarmTokenSupply
        getFarmingTokenId
        getFarmingTokenReserve
        getLastErrorMessage
        getLastRewardBlockNonce
        getLockedAssetFactoryManagedAddress
        getLockedRewardAprMuliplier
        getMinimumFarmingEpoch
        getOwner
        getPairContractManagedAddress
        getPenaltyPercent
        getPerBlockRewardAmount
        getRewardPerShare
        getRewardReserve
        getRewardTokenId
        getRouterManagedAddress
        getState
        getTransferExecGasLimit
        getUndistributedFees
        mergeFarmTokens
        pause
        registerFarmToken
        resume
        setFarmMigrationConfig
        setLocalRolesFarmToken
        setPerBlockRewardAmount
        set_locked_rewards_apr_multiplier
        set_minimum_farming_epochs
        set_penalty_percent
        set_transfer_exec_gas_limit
        start_produce_rewards
        stopRewardsAndMigrateRps
    )
}
