module DiemFramework::NFT {
    use Std::Event;
    use Std::GUID;
    use Std::Option::{Self, Option};
    use Std::Signer;
    use Std::Vector;

    /// Struct representing data of a specific token with token_id,
    /// stored under the creator's address inside TokenDataCollection.
    /// For each token_id, there is only one TokenData.
    struct TokenData<Type: store> has key, store {
        metadata: Option<Type>,
        /// Identifier for the token.
        token_id: GUID::GUID,
        /// Pointer to where the content and metadata is stored.
        content_uri: vector<u8>,
        supply: u64,
        /// Parent NFT id
        parent_id: Option<GUID::ID>
    }

    /// A hot potato wrapper for the token's metadata. Since this wrapper has no `key` or `store`
    /// ability, it can't be stored in global storage. This wrapper can be safely passed outside
    /// of this module because we know it will have to come back to this module, where
    /// it will be unpacked.
    struct TokenDataWrapper<Type: store> {
        origin: address,
        index: u64,
        metadata: Type,
        parent_id: Option<GUID::ID>,
    }

    /// Struct representing a semi-fungible or non-fungible token (depending on the supply).
    /// There can be multiple tokens with the same id (unless supply is 1). Each token's
    /// corresponding token metadata is stored inside a TokenData inside TokenDataCollection
    /// under the creator's address.
    struct Token<phantom Type: store> has key, store {
        id: GUID::ID,
        balance: u64,
    }

    struct MintEvent has copy, drop, store {
        id: GUID::ID,
        creator: address,
        content_uri: vector<u8>,
        amount: u64,
    }

    struct Admin has key {
        mint_events: Event::EventHandle<MintEvent>,
    }

    struct TokenDataCollection<Type: store> has key {
        tokens: vector<TokenData<Type>>,
    }

    const ADMIN: address = @0xa550c18;
    const MAX_U64: u64 = 18446744073709551615u64;
    // Error codes
    /// Function can only be called by the admin address
    const ENOT_ADMIN: u64  = 0;
    const EWRONG_TOKEN_ID: u64 = 1;
    const ETOKEN_BALANCE_OVERFLOWS: u64 = 2;
    const EAMOUNT_EXCEEDS_TOKEN_BALANCE: u64 = 3;
    const ETOKEN_EXTRACTED: u64 = 4;
    const EINDEX_EXCEEDS_LENGTH: u64 = 5;
    const ETOKEN_PRESENT: u64 = 6;
    const EPARENT_NOT_SAME_ACCOUNT: u64 = 7;
    const ETOKEN_DATA_COLLECTION_ALREADY_PUBLISHED: u64 = 8;

    public fun initialize<Type: store + drop>(account: &signer) {
        assert!(Signer::address_of(account) == ADMIN, ENOT_ADMIN);
        move_to(account, Admin { mint_events: Event::new_event_handle<MintEvent<Type>>(account) })
    }

    /// Returns the balance of given token
    public fun balance<Type: store>(token: &Token<Type>): u64 {
        token.balance
    }

    public fun metadata<Type: store>(wrapper: &TokenDataWrapper<Type>): &Type {
        &wrapper.metadata
    }

    /// Returns ID of collection associated with token
    public fun parent<Type: store>(wrapper: &TokenDataWrapper<Type>): &Option<GUID::ID> {
        &wrapper.parent_id
    }

    /// Returns the supply of tokens with `id` on the chain.
    public fun supply<Type: store>(id: &GUID::ID): u64 acquires TokenDataCollection {
        let owner_addr = GUID::id_creator_address(id);
        let tokens = &mut borrow_global_mut<TokenDataCollection<Type>>(owner_addr).tokens;
        let index_opt = index_of_token<Type>(tokens, id);
        assert(Option::is_some(&index_opt), Errors::invalid_argument(EWRONG_TOKEN_ID));
        let index = Option::extract(&mut index_opt);
        Vector::borrow(tokens, index).supply
    }

    /// Extract the Token data of the given token into a hot potato wrapper.
    public fun extract_token<Type: store>(nft: &Token<Type>): TokenDataWrapper<Type> acquires TokenDataCollection {
        let owner_addr = GUID::id_creator_address(&nft.id);
        let tokens = &mut borrow_global_mut<TokenDataCollection<Type>>(owner_addr).tokens;
        let index_opt = index_of_token<Type>(tokens, &nft.id);
        assert(Option::is_some(&index_opt), Errors::invalid_argument(EWRONG_TOKEN_ID));
        let index = Option::extract(&mut index_opt);
        let item_opt = &mut Vector::borrow_mut(tokens, index).metadata;
        assert(Option::is_some(item_opt), Errors::invalid_state(ETOKEN_EXTRACTED));
        let metadata = Option::extract(item_opt);
        let parent_opt = &mut Vector::borrow_mut(tokens, index).parent_id;
        TokenDataWrapper { origin: owner_addr, index, metadata, parent_id: *parent_opt }
    }

    /// Restore the token in the wrapper back into global storage under original address.
    public fun restore_token<Type: store>(wrapper: TokenDataWrapper<Type>) acquires TokenDataCollection {
        let TokenDataWrapper { origin, index, metadata, parent_id: _ } = wrapper;
        let tokens = &mut borrow_global_mut<TokenDataCollection<Type>>(origin).tokens;
        assert(Vector::length(tokens) > index, EINDEX_EXCEEDS_LENGTH);
        let item_opt = &mut Vector::borrow_mut(tokens, index).metadata;
        assert(Option::is_none(item_opt), ETOKEN_PRESENT);
        Option::fill(item_opt, metadata);
    }

    /// Finds the index of token with the given id in the gallery.
    fun index_of_token<Type: store>(gallery: &vector<TokenData<Type>>, id: &GUID::ID): Option<u64> {
        let i = 0;
        let len = Vector::length(gallery);
        while (i < len) {
            if (GUID::eq_id(&Vector::borrow(gallery, i).token_id, id)) {
                return Option::some(i)
            };
            i = i + 1;
        };
        Option::none()
    }

    /// Join two tokens and return a new token with the combined value of the two.
    public fun join<Type: store>(token: &mut Token<Type>, other: Token<Type>) {
        let Token { id, balance } = other;
        assert(*&token.id == id, EWRONG_TOKEN_ID);
        assert(MAX_U64 - token.balance >= balance, ETOKEN_BALANCE_OVERFLOWS);
        token.balance = token.balance + balance
    }

    /// Split the token into two tokens, one with balance `amount` and the other one with balance
    public fun split<Type: store>(token: Token<Type>, amount: u64): (Token<Type>, Token<Type>) {
        assert(token.balance >= amount, EAMOUNT_EXCEEDS_TOKEN_BALANCE);
        token.balance = token.balance - amount;
        let id = *&token.id;
        (token,
        Token {
            id,
            balance: amount
        } )
    }

    /// Initialize this module, to be called in genesis.
    public fun initialize(account: signer) {
        assert(Signer::address_of(&account) == ADMIN, ENOT_ADMIN);
        move_to(&account, Admin {
            mint_events: Event::new_event_handle<MintEvent>(&account),
        })
    }

    /// Create a` TokenData<Type>` that wraps `metadata` and with balance of `amount`
    public fun create<Type: store>(
        account: &signer, metadata: Type, content_uri: vector<u8>, amount: u64, parent_id: Option<GUID::ID>
    ): Token<Type> acquires Admin, TokenDataCollection {
        let guid = GUID::create(account);

        // If there is a parent, ensure it has the same creator
        // TODO: Do we just say the owner has the ability instead?
        if (Option::is_some(&parent_id)) {
            let parent_id = Option::borrow(&mut parent_id);
            assert(GUID::creator_address(&guid) == GUID::id_creator_address(parent_id), EPARENT_NOT_SAME_ACCOUNT);
        };
        Event::emit_event(
            &mut borrow_global_mut<Admin>(ADMIN).mint_events,
            MintEvent {
                id: GUID::id(&guid),
                creator: Signer::address_of(account),
                content_uri: copy content_uri,
                amount,
            }
        );
        let id = GUID::id(&guid);
        if (!exists<TokenDataCollection<Type>>(Signer::address_of(account))) {
            move_to(account, TokenDataCollection { tokens: Vector::empty<TokenData<Type>>() });
        };
        let token_data_collection = &mut borrow_global_mut<TokenDataCollection<Type>>(Signer::address_of(account)).tokens;
        Vector::push_back(
            token_data_collection,
            TokenData { metadata: Option::some(metadata), token_id: guid, content_uri, supply: amount, parent_id }
        );
        Token { id, balance: amount }
    }

    public fun publish_token_data_collection<Type: store>(account: &signer) {
        assert(
            !exists<TokenDataCollection<Type>>(Signer::address_of(account)),
            ETOKEN_DATA_COLLECTION_ALREADY_PUBLISHED
        );
        move_to(account, TokenDataCollection<Type> { tokens: Vector::empty() });
    }
}
}
