#![no_std]
#![allow(non_snake_case)]

elrond_wasm::imports!();
elrond_wasm::derive_imports!();

type Epoch = u64;
const PENALTY_PRECENT: u64 = 10;
const EXTERN_QUERY_MAX_GAS: u64 = 20000000;
const EXIT_FARM_NO_PENALTY_MIN_EPOCHS: u64 = 3;

pub mod liquidity_pool;
pub use crate::liquidity_pool::*;

#[derive(TopEncode, TopDecode, TypeAbi)]
pub struct FarmTokenAttributes<BigUint: BigUintApi> {
    farmed_token_id: TokenIdentifier,
    total_farmed_tokens: BigUint,
    total_initial_worth: BigUint,
    total_amount_liquidity: BigUint,
    epoch_when_entering: Epoch,
}

#[derive(TopEncode, TopDecode, PartialEq, TypeAbi)]
pub struct TokenAmountPair<BigUint: BigUintApi> {
    token_id: TokenIdentifier,
    amount: BigUint,
}

#[elrond_wasm_derive::callable(PairContractProxy)]
pub trait PairContract {
    fn getTokensForGivenPosition(
        &self,
        amount: BigUint,
    ) -> ContractCall<BigUint, MultiResult2<TokenAmountPair<BigUint>, TokenAmountPair<BigUint>>>;
    fn getEquivalent(
        &self,
        token: TokenIdentifier,
        amount: BigUint,
    ) -> ContractCall<BigUint, BigUint>;
}

#[elrond_wasm_derive::contract(FarmImpl)]
pub trait Farm {
    #[module(LiquidityPoolModuleImpl)]
    fn liquidity_pool(&self) -> LiquidityPoolModuleImpl<T, BigInt, BigUint>;

    #[init]
    fn init(
        &self,
        farming_pool_token_id: TokenIdentifier,
        router_address: Address,
        farm_with_lp_tokens: bool,
    ) {
        self.farming_pool_token_id().set(&farming_pool_token_id);
        self.router_address().set(&router_address);
        self.state().set(&true);
        self.owner().set(&self.blockchain().get_caller());
        self.farm_with_lp_tokens().set(&farm_with_lp_tokens);
    }

    #[endpoint]
    fn pause(&self) -> SCResult<()> {
        sc_try!(self.require_permissions());
        self.state().set(&false);
        Ok(())
    }

    #[endpoint]
    fn resume(&self) -> SCResult<()> {
        sc_try!(self.require_permissions());
        self.state().set(&true);
        Ok(())
    }

    #[endpoint(addTrustedPairAsOracle)]
    fn add_oracle_pair(
        &self,
        first_token: TokenIdentifier,
        second_token: TokenIdentifier,
        address: Address,
    ) -> SCResult<()> {
        require!(self.is_active(), "Not active");
        sc_try!(self.require_permissions());
        require!(self.farm_with_lp_tokens().get(), "Not an LP token farm");
        require!(
            self.oracle_pair(&first_token, &second_token).is_empty(),
            "Pair already exists as oracle for given tokens"
        );
        require!(
            self.oracle_pair(&second_token, &first_token).is_empty(),
            "Pair already exists as oracle for given tokens"
        );
        self.oracle_pair(&first_token, &second_token).set(&address);
        self.oracle_pair(&second_token, &first_token).set(&address);
        Ok(())
    }

    #[endpoint(removeTrustedPairAsOracle)]
    fn remove_oracle_pair(
        &self,
        first_token: TokenIdentifier,
        second_token: TokenIdentifier,
        address: Address,
    ) -> SCResult<()> {
        require!(self.is_active(), "Not active");
        sc_try!(self.require_permissions());
        require!(self.farm_with_lp_tokens().get(), "Not an LP token farm");
        require!(
            !self.oracle_pair(&first_token, &second_token).is_empty(),
            "Pair doesn't exists as oracle for given tokens"
        );
        require!(
            !self.oracle_pair(&second_token, &first_token).is_empty(),
            "Pair doesn't exists as oracle for given tokens"
        );
        require!(
            self.oracle_pair(&second_token, &first_token).get() == address,
            "Pair oracle has diferent address"
        );
        require!(
            self.oracle_pair(&first_token, &second_token).get() == address,
            "Pair oracle has diferent address"
        );
        self.oracle_pair(&first_token, &second_token).clear();
        Ok(())
    }

    #[endpoint(addAcceptedPairAddressAndLpToken)]
    fn add_accepted_pair(&self, address: Address, token: TokenIdentifier) -> SCResult<()> {
        require!(self.is_active(), "Not active");
        sc_try!(self.require_permissions());
        require!(self.farm_with_lp_tokens().get(), "Not an LP token farm");
        require!(address != Address::zero(), "Zero Address");
        require!(token.is_esdt(), "Not an ESDT token");
        require!(
            !self.pair_address_for_accepted_lp_token().contains_key(&token),
            "Pair address already exists for LP token"
        );
        self.pair_address_for_accepted_lp_token().insert(token, address);
        Ok(())
    }

    #[endpoint(removeAcceptedPairAddressAndLpToken)]
    fn remove_accepted_pair(&self, address: Address, token: TokenIdentifier) -> SCResult<()> {
        require!(self.is_active(), "Not active");
        sc_try!(self.require_permissions());
        require!(self.farm_with_lp_tokens().get(), "Not an LP token farm");
        require!(address != Address::zero(), "Zero Address");
        require!(token.is_esdt(), "Not an ESDT token");
        require!(
            self.pair_address_for_accepted_lp_token().contains_key(&token),
            "No Pair Address for given LP token"
        );
        require!(
            self.pair_address_for_accepted_lp_token().get(&token).unwrap() == address,
            "Address does not match Lp token equivalent"
        );
        self.pair_address_for_accepted_lp_token().remove(&token);
        Ok(())
    }

    #[payable("*")]
    #[endpoint(enterFarm)]
    fn enter_farm(
        &self,
        #[payment_token] token_in: TokenIdentifier,
        #[payment] amount: BigUint,
    ) -> SCResult<()> {
        require!(self.is_active(), "Not active");
        require!(!self.farm_token_id().is_empty(), "No issued farm token");
        let farm_contribution = sc_try!(self.get_farm_contribution(&token_in, &amount));
        require!(
            farm_contribution > BigUint::zero(),
            "Cannot farm with amount of 0"
        );

        let farming_pool_token_id = self.farming_pool_token_id().get();
        let liquidity = sc_try!(self.liquidity_pool().add_liquidity(
            farm_contribution.clone(),
            farming_pool_token_id,
            token_in.clone()
        ));
        let farm_attributes = FarmTokenAttributes::<BigUint> {
            farmed_token_id: token_in,
            total_farmed_tokens: amount,
            total_initial_worth: farm_contribution,
            total_amount_liquidity: liquidity.clone(),
            epoch_when_entering: self.blockchain().get_block_epoch(),
        };

        // This 1 is necessary to get_esdt_token_data needed for calculateRewardsForGivenPosition
        let farm_tokens_to_create = liquidity.clone() + BigUint::from(1u64);
        let farm_token_id = self.farm_token_id().get();
        self.create_farm_tokens(&farm_token_id, &farm_tokens_to_create, &farm_attributes);
        let farm_token_nonce = self.blockchain().get_current_esdt_nft_nonce(
            &self.blockchain().get_sc_address(),
            farm_token_id.as_esdt_identifier(),
        );

        let _ = self.send().direct_esdt_nft_via_transfer_exec(
            &self.blockchain().get_caller(),
            farm_token_id.as_esdt_identifier(),
            farm_token_nonce,
            &liquidity,
            &[],
        );

        Ok(())
    }

    #[payable("*")]
    #[endpoint(exitFarm)]
    fn exit_farm(&self) -> SCResult<()> {
        //require!(self.is_active(), "Not active");
        require!(!self.farm_token_id().is_empty(), "No issued farm token");
        let (liquidity, payment_token_id) = self.call_value().payment_token_pair();
        let token_nonce = self.call_value().esdt_token_nonce();
        let farm_token_id = self.farm_token_id().get();
        require!(payment_token_id == farm_token_id, "Unknown farm token");

        let farm_attributes =
            sc_try!(self.get_farm_attributes(payment_token_id.clone(), token_nonce));
        let initial_worth = farm_attributes.total_initial_worth.clone() * liquidity.clone()
            / farm_attributes.total_amount_liquidity.clone();
        require!(initial_worth > 0, "Cannot unfarm with 0 intial_worth");
        let farmed_token_amount = farm_attributes.total_farmed_tokens.clone() * liquidity.clone()
            / farm_attributes.total_amount_liquidity.clone();
        require!(farmed_token_amount > 0, "Cannot unfarm with 0 farmed_token");

        let farming_pool_token_id = self.farming_pool_token_id().get();
        let reward = sc_try!(self.liquidity_pool().remove_liquidity(
            liquidity.clone(),
            initial_worth,
            farming_pool_token_id.clone(),
            farm_attributes.farmed_token_id.clone(),
        ));
        self.burn(&payment_token_id, token_nonce, &liquidity);

        let (reward_to_send, farmed_token_to_send) =
            if self.should_apply_penalty(farm_attributes.epoch_when_entering) {
                (
                    self.apply_penalty(reward),
                    self.apply_penalty(farmed_token_amount),
                )
            } else {
                (reward, farmed_token_amount)
            };

        let caller = self.blockchain().get_caller();
        if reward_to_send != 0 {
            let _ = self.send().direct_esdt_via_transf_exec(
                &caller,
                farming_pool_token_id.as_esdt_identifier(),
                &reward_to_send,
                &[],
            );
        }

        if farmed_token_to_send != 0 {
            let _ = self.send().direct_esdt_via_transf_exec(
                &caller,
                farm_attributes.farmed_token_id.as_esdt_identifier(),
                &farmed_token_to_send,
                &[],
            );
        }

        Ok(())
    }

    #[view(calculateRewardsForGivenPosition)]
    fn calculate_rewards_for_given_position(
        &self,
        token_nonce: u64,
        liquidity: BigUint,
    ) -> SCResult<BigUint> {
        let token_id = self.farm_token_id().get();
        let token_current_nonce = self.blockchain().get_current_esdt_nft_nonce(
            &self.blockchain().get_sc_address(),
            token_id.as_esdt_identifier(),
        );
        require!(token_nonce <= token_current_nonce, "Invalid nonce");

        let attributes = sc_try!(self.get_farm_attributes(token_id, token_nonce));
        let initial_worth = attributes.total_initial_worth.clone() * liquidity.clone()
            / attributes.total_amount_liquidity;
        if initial_worth == 0 {
            return Ok(initial_worth);
        }

        let reward = sc_try!(self.liquidity_pool().calculate_reward(
            liquidity,
            initial_worth,
            self.farming_pool_token_id().get(),
        ));

        if self.should_apply_penalty(attributes.epoch_when_entering) {
            Ok(self.apply_penalty(reward))
        } else {
            Ok(reward)
        }
    }

    #[payable("EGLD")]
    #[endpoint(issueFarmToken)]
    fn issue_farm_token(
        &self,
        #[payment] issue_cost: BigUint,
        token_display_name: BoxedBytes,
        token_ticker: BoxedBytes,
    ) -> SCResult<AsyncCall<BigUint>> {
        require!(self.is_active(), "Not active");
        sc_try!(self.require_permissions());
        require!(self.farm_token_id().is_empty(), "Already issued");

        Ok(self.issue_token(issue_cost, token_display_name, token_ticker))
    }

    fn issue_token(
        &self,
        issue_cost: BigUint,
        token_display_name: BoxedBytes,
        token_ticker: BoxedBytes,
    ) -> AsyncCall<BigUint> {
        ESDTSystemSmartContractProxy::new()
            .issue_semi_fungible(
                issue_cost,
                &token_display_name,
                &token_ticker,
                SemiFungibleTokenProperties {
                    can_freeze: true,
                    can_wipe: true,
                    can_pause: true,
                    can_change_owner: true,
                    can_upgrade: true,
                    can_add_special_roles: true,
                },
            )
            .async_call()
            .with_callback(
                self.callbacks()
                    .issue_callback(&self.blockchain().get_caller()),
            )
    }

    #[callback]
    fn issue_callback(
        &self,
        caller: &Address,
        #[call_result] result: AsyncCallResult<TokenIdentifier>,
    ) {
        match result {
            AsyncCallResult::Ok(token_id) => {
                if self.farm_token_id().is_empty() {
                    self.farm_token_id().set(&token_id);
                }
            }
            AsyncCallResult::Err(_) => {
                let (returned_tokens, token_id) = self.call_value().payment_token_pair();
                if token_id.is_egld() && returned_tokens > 0 {
                    let _ = self.send().direct_egld(caller, &returned_tokens, &[]);
                }
            }
        }
    }

    #[endpoint(setLocalRolesFarmToken)]
    fn set_local_roles_farm_token(&self) -> SCResult<AsyncCall<BigUint>> {
        require!(self.is_active(), "Not active");
        sc_try!(self.require_permissions());
        require!(!self.farm_token_id().is_empty(), "No farm token issued");

        let token = self.farm_token_id().get();
        Ok(self.set_local_roles(token))
    }

    fn set_local_roles(&self, token: TokenIdentifier) -> AsyncCall<BigUint> {
        ESDTSystemSmartContractProxy::new()
            .set_special_roles(
                &self.blockchain().get_sc_address(),
                token.as_esdt_identifier(),
                &[
                    EsdtLocalRole::NftCreate,
                    EsdtLocalRole::NftAddQuantity,
                    EsdtLocalRole::NftBurn,
                ],
            )
            .async_call()
            .with_callback(self.callbacks().change_roles_callback())
    }

    #[callback]
    fn change_roles_callback(&self, #[call_result] result: AsyncCallResult<()>) {
        match result {
            AsyncCallResult::Ok(()) => {
                self.last_error_message().clear();
            }
            AsyncCallResult::Err(message) => {
                self.last_error_message().set(&message.err_msg);
            }
        }
    }

    fn get_farm_attributes(
        &self,
        token_id: TokenIdentifier,
        token_nonce: u64,
    ) -> SCResult<FarmTokenAttributes<BigUint>> {
        let token_info = self.blockchain().get_esdt_token_data(
            &self.blockchain().get_sc_address(),
            token_id.as_esdt_identifier(),
            token_nonce,
        );

        let farm_attributes = token_info.decode_attributes::<FarmTokenAttributes<BigUint>>();
        match farm_attributes {
            Result::Ok(decoded_obj) => Ok(decoded_obj),
            Result::Err(_) => {
                return sc_error!("Decoding error");
            }
        }
    }

    fn create_farm_tokens(
        &self,
        token_id: &TokenIdentifier,
        amount: &BigUint,
        attributes: &FarmTokenAttributes<BigUint>,
    ) {
        self.send().esdt_nft_create::<FarmTokenAttributes<BigUint>>(
            self.blockchain().get_gas_left(),
            token_id.as_esdt_identifier(),
            amount,
            &BoxedBytes::empty(),
            &BigUint::zero(),
            &H256::zero(),
            attributes,
            &[BoxedBytes::empty()],
        );
    }

    fn burn(&self, token: &TokenIdentifier, nonce: u64, amount: &BigUint) {
        self.send().esdt_nft_burn(
            self.blockchain().get_gas_left(),
            token.as_esdt_identifier(),
            nonce,
            amount,
        );
    }

    fn require_permissions(&self) -> SCResult<()> {
        let caller = self.blockchain().get_caller();
        let owner = self.owner().get();
        let router = self.router_address().get();
        require!(caller == owner || caller == router, "Permission denied");
        Ok(())
    }

    #[view(getFarmContribution)]
    fn get_farm_contribution(
        &self,
        token_in: &TokenIdentifier,
        amount_in: &BigUint,
    ) -> SCResult<BigUint> {
        let farming_pool_token_id = self.farming_pool_token_id().get();
        if &farming_pool_token_id == token_in && !self.farm_with_lp_tokens().get() {
            return Ok(amount_in.clone());
        }
        require!(
            self.pair_address_for_accepted_lp_token().contains_key(&token_in),
            "Unknown LP token"
        );
        let pair = self.pair_address_for_accepted_lp_token().get(&token_in).unwrap();

        let mut gas_limit = core::cmp::min(self.blockchain().get_gas_left(), EXTERN_QUERY_MAX_GAS);
        let equivalent = contract_call!(self, pair, PairContractProxy)
            .getTokensForGivenPosition(amount_in.clone())
            .execute_on_dest_context(gas_limit, self.send());

        let token_amount_pair_tuple = equivalent.0;
        let first_token_amount_pair = token_amount_pair_tuple.0;
        let second_token_amount_pair = token_amount_pair_tuple.1;

        if first_token_amount_pair.token_id == farming_pool_token_id {
            return Ok(first_token_amount_pair.amount);
        } else if second_token_amount_pair.token_id == farming_pool_token_id {
            return Ok(second_token_amount_pair.amount);
        }

        let (token_to_ask, oracle_pair_to_ask) = if !self
            .oracle_pair(&first_token_amount_pair.token_id, &farming_pool_token_id)
            .is_empty()
        {
            (
                first_token_amount_pair.token_id.clone(),
                self.oracle_pair(&first_token_amount_pair.token_id, &farming_pool_token_id)
                    .get(),
            )
        } else if !self
            .oracle_pair(&second_token_amount_pair.token_id, &farming_pool_token_id)
            .is_empty()
        {
            (
                second_token_amount_pair.token_id.clone(),
                self.oracle_pair(&second_token_amount_pair.token_id, &farming_pool_token_id)
                    .get(),
            )
        } else {
            return sc_error!("Cannot get a farming equivalent for given tokens");
        };

        gas_limit = core::cmp::min(self.blockchain().get_gas_left(), EXTERN_QUERY_MAX_GAS);
        Ok(contract_call!(self, oracle_pair_to_ask, PairContractProxy)
            .getEquivalent(token_to_ask, amount_in.clone())
            .execute_on_dest_context(gas_limit, self.send()))
    }

    #[inline]
    fn should_apply_penalty(&self, entering_epoch: Epoch) -> bool {
        entering_epoch + EXIT_FARM_NO_PENALTY_MIN_EPOCHS >= self.blockchain().get_block_epoch()
    }

    #[inline]
    fn apply_penalty(&self, amount: BigUint) -> BigUint {
        amount * BigUint::from(100 - PENALTY_PRECENT) / BigUint::from(100u64)
    }

    #[inline]
    fn is_active(&self) -> bool {
        self.state().get()
    }

    #[view(getFarmingPoolTokenIdAndAmounts)]
    fn get_farming_pool_token_id_and_amounts(
        &self,
    ) -> SCResult<(TokenIdentifier, (BigUint, BigUint))> {
        require!(!self.farming_pool_token_id().is_empty(), "Not issued");
        let token = self.farming_pool_token_id().get();
        let vamount = self.liquidity_pool().virtual_reserves().get();
        let amount = self.blockchain().get_esdt_balance(
            &self.blockchain().get_sc_address(),
            token.as_esdt_identifier(),
            0,
        );
        Ok((token, (vamount, amount)))
    }

    #[view(getAllAcceptedTokens)]
    fn get_all_accepted_tokens(&self) -> MultiResultVec<TokenIdentifier> {
        if self.farm_with_lp_tokens().get() {
            self.pair_address_for_accepted_lp_token().keys().collect()
        } else {
            let mut result = MultiResultVec::<TokenIdentifier>::new();
            result.push(self.farming_pool_token_id().get());
            result
        }
    }

    #[storage_mapper("pair_address_for_accepted_lp_token")]
    fn pair_address_for_accepted_lp_token(
        &self,
    ) -> MapMapper<Self::Storage, TokenIdentifier, Address>;

    #[storage_mapper("oracle_pair")]
    fn oracle_pair(
        &self,
        first_token_id: &TokenIdentifier,
        second_token_id: &TokenIdentifier,
    ) -> SingleValueMapper<Self::Storage, Address>;

    #[view(getFarmingPoolTokenId)]
    #[storage_mapper("farming_pool_token_id")]
    fn farming_pool_token_id(&self) -> SingleValueMapper<Self::Storage, TokenIdentifier>;

    #[view(getFarmTokenId)]
    #[storage_mapper("farm_token_id")]
    fn farm_token_id(&self) -> SingleValueMapper<Self::Storage, TokenIdentifier>;

    #[view(getLastErrorMessage)]
    #[storage_mapper("last_error_message")]
    fn last_error_message(&self) -> SingleValueMapper<Self::Storage, BoxedBytes>;

    #[view(getRouterAddress)]
    #[storage_mapper("router_address")]
    fn router_address(&self) -> SingleValueMapper<Self::Storage, Address>;

    #[view(getState)]
    #[storage_mapper("state")]
    fn state(&self) -> SingleValueMapper<Self::Storage, bool>;

    #[view(getOwner)]
    #[storage_mapper("owner")]
    fn owner(&self) -> SingleValueMapper<Self::Storage, Address>;

    #[storage_mapper("farm_with_lp_tokens")]
    fn farm_with_lp_tokens(&self) -> SingleValueMapper<Self::Storage, bool>;
}
