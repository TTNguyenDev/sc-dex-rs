#![no_std]

elrond_wasm::imports!();
elrond_wasm::derive_imports!();



pub mod liquidity_pool;
pub use crate::liquidity_pool::*;

// used as mock attributes for NFTs
#[derive(TopEncode, TopDecode, TypeAbi)]
pub struct SFTAttributes<BigUint: BigUintApi> {
	lp_token_id: TokenIdentifier,
	total_lp_tokens: BigUint,
	total_initial_worth: BigUint,
	total_amount_liquidity: BigUint
}

#[elrond_wasm_derive::callable(PairContractProxy)]
pub trait PairContract {
	fn get_tokens_for_given_position(&self, amount: BigUint) -> ContractCall<BigUint>;
}

#[elrond_wasm_derive::contract(StakingImpl)]
pub trait Staking {

	#[module(LiquidityPoolModuleImpl)]
	fn liquidity_pool(&self) -> LiquidityPoolModuleImpl<T, BigInt, BigUint>;

	#[init]
	fn init(&self, wegld_token_identifier: TokenIdentifier) {
		self.wegld_token_identifier().set(&wegld_token_identifier);
		self.liquidity_pool().virtual_token_id().set(&wegld_token_identifier);
	}

	#[payable("*")]
	#[endpoint(stake)]
	fn stake(
		&self,
		#[payment_token] lp_token: TokenIdentifier,
		#[payment] amount: BigUint,
	) -> SCResult<()> {

		let pair = self.get_pair_for_lp_token(&lp_token);
		require!(pair != Address::zero(), "Unknown lp token");

		//TODO: Ask get_tokens_for_given_position 	with execute on dest context
		let wegld_amount = BigUint::zero();

		let liquidity = sc_try!(self.liquidity_pool().add_liquidity(wegld_amount.clone()));

		let attributes = SFTAttributes{
			lp_token_id: lp_token.clone(),
			total_lp_tokens: amount.clone(),
			total_initial_worth: wegld_amount.clone(),
			total_amount_liquidity: liquidity.clone()
		};

		//TODO: Create STF with amount $(liquidity) with $(attributes) and send
		Ok(())
	}

	#[payable("*")]
	#[endpoint(unstake)]
	fn unstake(
		&self,
		#[payment_token] staking_token: TokenIdentifier,
		#[payment] liquidity: BigUint,
	) -> SCResult<()> {

		let sft_id = self.sft_staking_token_identifier().get();
		require!(staking_token == sft_id, "Unknown staking token");

		//TODO: Add actual read of attributes
		let attributes = SFTAttributes{
			lp_token_id: staking_token.clone(),
			total_lp_tokens: liquidity.clone(),
			total_initial_worth: liquidity.clone(),
			total_amount_liquidity: liquidity.clone()
		};

		let pair = self.get_pair_for_lp_token(&attributes.lp_token_id);
		require!(pair != Address::zero(), "Unknown lp token");

		let initial_worth = attributes.total_initial_worth.clone() * liquidity.clone() / 
			attributes.total_amount_liquidity.clone();
		require!(initial_worth > 0, "Cannot unstake with intial_worth == 0");
		let lp_tokens = attributes.total_lp_tokens.clone() * liquidity.clone() / 
			attributes.total_amount_liquidity.clone();
		require!(lp_tokens > 0, "Cannot unstake with lp_tokens == 0");

		let reward = sc_try!(self.liquidity_pool().remove_liquidity(liquidity.clone(), initial_worth.clone()));
		if reward != BigUint::zero() {
			let wegld_balance = self.get_esdt_balance(
				&self.get_sc_address(),
				self.wegld_token_identifier().get().as_esdt_identifier(),
				0,
			);
			//TODO: Add invariant. Something went really wrong.
			require!(wegld_balance > reward, "Not enough funds");

			self.send().direct_esdt_via_transf_exec(
				&self.get_caller(),
				self.wegld_token_identifier().get().as_esdt_identifier(),
				&reward,
				&[]
			);
		}

		//Burn SFT $(liquidity) tokens with type $(staking_token) + add invariant + require!

		let mut unstake_amount = self.get_unstake_amount(&self.get_caller(), &attributes.lp_token_id);
		unstake_amount += lp_tokens;
		self.set_unstake_amount(&self.get_caller(), &attributes.lp_token_id, &unstake_amount);
		self.set_unbond_epoch(&self.get_caller(), &attributes.lp_token_id, self.get_block_epoch() + 14400); //10 days

		Ok(())
	}

	#[endpoint(unbond)]
	fn unbond(
		&self,
		lp_token: TokenIdentifier
	) -> SCResult<()> {

		let caller = self.get_caller();
		require!(!self.is_empty_unstake_amount(&caller, &lp_token), "Don't have anything to unbond");
		let block_epoch = self.get_block_epoch();
		let unbond_epoch = self.get_unbond_epoch(&self.get_caller(), &lp_token);
		require!(block_epoch >= unbond_epoch, "Unbond called too early");

		let unstake_amount = self.get_unstake_amount(&self.get_caller(), &lp_token);
		let lp_token_balance = self.get_esdt_balance(
			&self.get_sc_address(),
			lp_token.as_esdt_identifier(),
			0,
		);
		//TODO: Add invariant. Something went really wrong.
		require!(lp_token_balance > unstake_amount, "Not enough lp tokens");

		self.send().direct_esdt_via_transf_exec(
			&self.get_caller(),
			lp_token.as_esdt_identifier(),
			&unstake_amount,
			&[]
		);

		self.clear_unstake_amount(&caller, &lp_token);
		self.clear_unbond_epoch(&caller, &lp_token);
		Ok(())
	}

	#[payable("EGLD")]
	#[endpoint(sftIssue)]
	fn sft_issue(
		&self,
		#[payment] issue_cost: BigUint,
		token_display_name: BoxedBytes,
		token_ticker: BoxedBytes,
	) -> SCResult<AsyncCall<BigUint>> {

		only_owner!(self, "Permission denied");
		if !self.sft_staking_token_identifier().is_empty() {
			return sc_error!("Already issued");
		}

		let caller = self.get_caller();
		Ok(ESDTSystemSmartContractProxy::new()
			.issue_semi_fungible(
				issue_cost,
				&token_display_name,
				&token_ticker,
				true,
				true,
				true,
				true,
				true,
				true,
			)
			.async_call()
			.with_callback(self.callbacks().sft_issue_callback(&caller))
		)
	}

	#[callback]
	fn sft_issue_callback(
		&self,
		caller: &Address,
		#[call_result] result: AsyncCallResult<TokenIdentifier>,
	) {

		let mut success = false;
		match result {
			AsyncCallResult::Ok(token_identifier) => {
				if self.sft_staking_token_identifier().is_empty() {
					success = true;
					self.sft_staking_token_identifier().set(&token_identifier);
				}
			},
			AsyncCallResult::Err(_) => {
				success = false;
			},
		}

		if success == false {
			let (returned_tokens, token_identifier) = self.call_value().payment_token_pair();
			if token_identifier.is_egld() && returned_tokens > 0 {
				self.send().direct_egld(caller, &returned_tokens, &[]);
			}
		}
	}

	#[view(getPairForLpToken)]
	#[storage_get("pair_for_lp_token")]
	fn get_pair_for_lp_token(&self, lp_token: &TokenIdentifier) -> Address;

	#[storage_set("pair_for_lp_token")]
	fn set_pair_for_lp_token(&self, lp_token: &TokenIdentifier, pair_address: &Address);


	#[view(getLpTokenForPair)]
	#[storage_get("lp_token_for_pair")]
	fn get_lp_token_for_pair(&self, pair_address: &Address) -> TokenIdentifier;

	#[storage_set("lp_token_for_pair")]
	fn set_lp_token_for_pair(&self, pair_address: &Address, token: &TokenIdentifier);

	#[storage_is_empty("lp_token_for_pair")]
	fn is_empty_lp_token_for_pair(&self, pair_address: &Address) -> bool;


	#[view(getWegldTokenIdentifier)]
	#[storage_mapper("wegld_token_identifier")]
	fn wegld_token_identifier(&self) -> SingleValueMapper<Self::Storage, TokenIdentifier>;

	#[view(getSftStakingTokenIdentifier)]
	#[storage_mapper("sft_staking_token_identifier")]
	fn sft_staking_token_identifier(&self) -> SingleValueMapper<Self::Storage, TokenIdentifier>;


	#[storage_get("unbond_epoch")]
	fn get_unbond_epoch(&self, address: &Address, token: &TokenIdentifier) -> u64;

	#[storage_set("unbond_epoch")]
	fn set_unbond_epoch(&self, address: &Address, token: &TokenIdentifier, epoch: u64);

	#[storage_clear("unbond_epoch")]
	fn clear_unbond_epoch(&self, address: &Address, token: &TokenIdentifier);


	#[storage_get("unstake_amount")]
	fn get_unstake_amount(&self, address: &Address, token: &TokenIdentifier) -> BigUint;

	#[storage_set("unstake_amount")]
	fn set_unstake_amount(&self, address: &Address, token: &TokenIdentifier, amount: &BigUint);

	#[storage_clear("unstake_amount")]
	fn clear_unstake_amount(&self, address: &Address, token: &TokenIdentifier);
	
	#[storage_is_empty("unstake_amount")]
	fn is_empty_unstake_amount(&self, address: &Address, token: &TokenIdentifier) -> bool;
}

