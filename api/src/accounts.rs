// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    metrics::metrics,
    param::{AddressParam, LedgerVersionParam, MoveIdentifierParam, MoveStructTagParam},
};

use diem_api_types::{Address, Error, LedgerInfo, MoveModuleBytecode, Response, TransactionId};
use diem_types::{
    account_state::AccountState,
    event::{EventHandle, EventKey},
};

use anyhow::Result;
use move_core_types::{identifier::Identifier, language_storage::StructTag, value::MoveValue};
use std::convert::TryInto;
use warp::{Filter, Rejection, Reply};

pub fn routes(context: Context) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    get_account_resources(context.clone())
        .or(get_account_resources_by_ledger_version(context.clone()))
        .or(get_account_modules(context.clone()))
        .or(get_account_modules_by_ledger_version(context))
}

// GET /accounts/<address>/resources
pub fn get_account_resources(
    context: Context,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("accounts" / AddressParam / "resources")
        .and(warp::get())
        .and(context.filter())
        .map(|address, ctx| (None, address, ctx))
        .untuple_one()
        .and_then(handle_get_account_resources)
        .with(metrics("get_account_resources"))
}

// GET /ledger/<version>/accounts/<address>/resources
pub fn get_account_resources_by_ledger_version(
    context: Context,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("ledger" / LedgerVersionParam / "accounts" / AddressParam / "resources")
        .and(warp::get())
        .and(context.filter())
        .map(|version, address, ctx| (Some(version), address, ctx))
        .untuple_one()
        .and_then(handle_get_account_resources)
        .with(metrics("get_account_resources_by_ledger_version"))
}

// GET /accounts/<address>/modules
pub fn get_account_modules(
    context: Context,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("accounts" / AddressParam / "modules")
        .and(warp::get())
        .and(context.filter())
        .map(|address, ctx| (None, address, ctx))
        .untuple_one()
        .and_then(handle_get_account_modules)
        .with(metrics("get_account_modules"))
}

// GET /ledger/<version>/accounts/<address>/modules
pub fn get_account_modules_by_ledger_version(
    context: Context,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("ledger" / LedgerVersionParam / "accounts" / AddressParam / "modules")
        .and(warp::get())
        .and(context.filter())
        .map(|version, address, ctx| (Some(version), address, ctx))
        .untuple_one()
        .and_then(handle_get_account_modules)
        .with(metrics("get_account_modules_by_ledger_version"))
}

async fn handle_get_account_resources(
    ledger_version: Option<LedgerVersionParam>,
    address: AddressParam,
    context: Context,
) -> Result<impl Reply, Rejection> {
    Ok(Account::new(ledger_version, address, context)?.resources()?)
}

async fn handle_get_account_modules(
    ledger_version: Option<LedgerVersionParam>,
    address: AddressParam,
    context: Context,
) -> Result<impl Reply, Rejection> {
    Ok(Account::new(ledger_version, address, context)?.modules()?)
}

pub(crate) struct Account {
    ledger_version: u64,
    address: Address,
    latest_ledger_info: LedgerInfo,
    context: Context,
}

impl Account {
    pub fn new(
        ledger_version: Option<LedgerVersionParam>,
        address: AddressParam,
        context: Context,
    ) -> Result<Self, Error> {
        let latest_ledger_info = context.get_latest_ledger_info()?;
        let ledger_version = ledger_version
            .map(|v| v.parse("ledger version"))
            .unwrap_or_else(|| Ok(latest_ledger_info.version()))?;

        if ledger_version > latest_ledger_info.version() {
            return Err(Error::not_found(
                "ledger",
                TransactionId::Version(ledger_version),
                latest_ledger_info.version(),
            ));
        }

        Ok(Self {
            ledger_version,
            address: address.parse("account address")?,
            latest_ledger_info,
            context,
        })
    }

    pub fn resources(self) -> Result<impl Reply, Error> {
        let resources = self
            .context
            .move_converter()
            .try_into_resources(self.account_state()?.get_resources())?;
        Response::new(self.latest_ledger_info, &resources)
    }

    pub fn modules(self) -> Result<impl Reply, Error> {
        let modules = self
            .account_state()?
            .into_modules()
            .map(MoveModuleBytecode::new)
            .map(|m| m.try_parse_abi())
            .collect::<Result<Vec<MoveModuleBytecode>>>()?;
        Response::new(self.latest_ledger_info, &modules)
    }

    pub fn find_event_key(
        &self,
        struct_tag_param: MoveStructTagParam,
        field_name_param: MoveIdentifierParam,
    ) -> Result<EventKey, Error> {
        let struct_tag: StructTag = struct_tag_param.parse("event handle struct")?.try_into()?;
        let field_name = field_name_param.parse("event handle field name")?;

        let resource = self.find_resource(&struct_tag)?;

        let (_id, value) = resource
            .into_iter()
            .find(|(id, _)| id == &field_name)
            .ok_or_else(|| self.field_not_found(&struct_tag, &field_name))?;

        // serialization should not fail, otherwise it's internal bug
        let event_handle_bytes = bcs::to_bytes(&value).map_err(anyhow::Error::from)?;
        // deserialization may fail because the bytes are not EventHandle struct type.
        let event_handle: EventHandle = bcs::from_bytes(&event_handle_bytes).map_err(|e| {
            Error::bad_request(format!(
                "field({}) type is not EventHandle struct, deserialize error: {}",
                field_name, e
            ))
        })?;
        Ok(*event_handle.key())
    }

    pub fn find_resource(
        &self,
        struct_tag: &StructTag,
    ) -> Result<Vec<(Identifier, MoveValue)>, Error> {
        let account_state = self.account_state()?;
        let (typ, data) = account_state
            .get_resources()
            .find(|(tag, _data)| tag == struct_tag)
            .ok_or_else(|| self.resource_not_found(struct_tag))?;
        Ok(self
            .context
            .move_converter()
            .move_struct_fields(&typ, data)?)
    }

    fn account_state(&self) -> Result<AccountState, Error> {
        let state = self
            .context
            .get_account_state(self.address.into(), self.ledger_version)?
            .ok_or_else(|| self.account_not_found())?;
        Ok(state)
    }

    fn account_not_found(&self) -> Error {
        Error::not_found(
            "account",
            format!(
                "address({}) and ledger version({})",
                self.address, self.ledger_version,
            ),
            self.latest_ledger_info.version(),
        )
    }

    fn resource_not_found(&self, struct_tag: &StructTag) -> Error {
        Error::not_found(
            "resource",
            format!(
                "address({}), struct tag({}) and ledger version({})",
                self.address, struct_tag, self.ledger_version,
            ),
            self.latest_ledger_info.version(),
        )
    }

    fn field_not_found(&self, struct_tag: &StructTag, field_name: &Identifier) -> Error {
        Error::not_found(
            "resource",
            format!(
                "address({}), struct tag({}), field name({}) and ledger version({})",
                self.address, struct_tag, field_name, self.ledger_version,
            ),
            self.latest_ledger_info.version(),
        )
    }
}


// // Copyright (c) The Diem Core Contributors
// // SPDX-License-Identifier: Apache-2.0

// use crate::{
//     context::Context,
//     metrics::metrics,
//     param::{AddressParam, LedgerVersionParam, MoveIdentifierParam, MoveStructTagParam},
// };

// use diem_api_types::{Address, Error, LedgerInfo, MoveModuleBytecode, Response, TransactionId};
// use diem_types::{
//     account_state::AccountState,
//     event::{EventHandle, EventKey},
// };

// use anyhow::Result;
// use move_core_types::{identifier::Identifier, language_storage::StructTag, value::MoveValue};
// use std::convert::TryInto;
// use warp::{Filter, Rejection, Reply};

// pub fn routes(context: Context) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
//     get_account_resources(context.clone())
//         .or(get_account_resources_by_ledger_version(context.clone()))
//         .or(get_account_modules(context.clone()))
//         .or(get_account_modules_by_ledger_version(context))
// }

// // GET /accounts/<address>/resources
// pub fn get_account_resources(
//     context: Context,
// ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
//     warp::path!("accounts" / AddressParam / "resources")
//         .and(warp::get())
//         .and(context.filter())
//         .map(|address, ctx| (None, address, ctx))
//         .untuple_one()
//         .and_then(handle_get_account_resources)
//         .with(metrics("get_account_resources"))
// }

// // GET /ledger/<version>/accounts/<address>/resources
// pub fn get_account_resources_by_ledger_version(
//     context: Context,
// ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
//     warp::path!("ledger" / LedgerVersionParam / "accounts" / AddressParam / "resources")
//         .and(warp::get())
//         .and(context.filter())
//         .map(|version, address, ctx| (Some(version), address, ctx))
//         .untuple_one()
//         .and_then(handle_get_account_resources)
//         .with(metrics("get_account_resources_by_ledger_version"))
// }

// // GET /accounts/<address>/modules
// pub fn get_account_modules(
//     context: Context,
// ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
//     warp::path!("accounts" / AddressParam / "modules")
//         .and(warp::get())
//         .and(context.filter())
//         .map(|address, ctx| (None, address, ctx))
//         .untuple_one()
//         .and_then(handle_get_account_modules)
//         .with(metrics("get_account_modules"))
// }

// // GET /ledger/<version>/accounts/<address>/modules
// pub fn get_account_modules_by_ledger_version(
//     context: Context,
// ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
//     warp::path!("ledger" / LedgerVersionParam / "accounts" / AddressParam / "modules")
//         .and(warp::get())
//         .and(context.filter())
//         .map(|version, address, ctx| (Some(version), address, ctx))
//         .untuple_one()
//         .and_then(handle_get_account_modules)
//         .with(metrics("get_account_modules_by_ledger_version"))
// }

// async fn handle_get_account_resources(
//     ledger_version: Option<LedgerVersionParam>,
//     address: AddressParam,
//     context: Context,
// ) -> Result<impl Reply, Rejection> {
//     Ok(Account::new(ledger_version, address, context)?.resources()?)
// }

// async fn handle_get_account_modules(
//     ledger_version: Option<LedgerVersionParam>,
//     address: AddressParam,
//     context: Context,
// ) -> Result<impl Reply, Rejection> {
//     Ok(Account::new(ledger_version, address, context)?.modules()?)
// }

// pub(crate) struct Account {
//     ledger_version: u64,
//     address: Address,
//     latest_ledger_info: LedgerInfo,
//     context: Context,
// }

// impl Account {
//     pub fn new(
//         ledger_version: Option<LedgerVersionParam>,
//         address: AddressParam,
//         context: Context,
//     ) -> Result<Self, Error> {
//         let latest_ledger_info = context.get_latest_ledger_info()?;
//         let ledger_version = ledger_version
//             .map(|v| v.parse("ledger version"))
//             .unwrap_or_else(|| Ok(latest_ledger_info.version()))?;

//         if ledger_version > latest_ledger_info.version() {
//             return Err(Error::not_found(
//                 "ledger",
//                 TransactionId::Version(ledger_version),
//                 latest_ledger_info.version(),
//             ));
//         }

//         Ok(Self {
//             ledger_version,
//             address: address.parse("account address")?,
//             latest_ledger_info,
//             context,
//         })
//     }

//     pub fn resources(self) -> Result<impl Reply, Error> {
//         let resources = self
//             .context
//             .move_converter()
//             .try_into_resources(self.account_state()?.get_resources())?;
//         Response::new(self.latest_ledger_info, &resources)
//     }

//     pub fn modules(self) -> Result<impl Reply, Error> {
//         let modules = self
//             .account_state()?
//             .into_modules()
//             .map(MoveModuleBytecode::new)
//             .map(|m| m.try_parse_abi())
//             .collect::<Result<Vec<MoveModuleBytecode>>>()?;
//         Response::new(self.latest_ledger_info, &modules)
//     }

//     pub fn find_event_key(
//         &self,
//         struct_tag_param: MoveStructTagParam,
//         field_name_param: MoveIdentifierParam,
//     ) -> Result<EventKey, Error> {
//         let struct_tag: StructTag = struct_tag_param.parse("event handle struct")?.try_into()?;
//         let field_name = field_name_param.parse("event handle field name")?;

//         let resource = self.find_resource(&struct_tag)?;

//         let (_id, value) = resource
//             .into_iter()
//             .find(|(id, _)| id == &field_name)
//             .ok_or_else(|| self.field_not_found(&struct_tag, &field_name))?;

//         // serialization should not fail, otherwise it's internal bug
//         let event_handle_bytes = bcs::to_bytes(&value).map_err(anyhow::Error::from)?;
//         // deserialization may fail because the bytes are not EventHandle struct type.
//         let event_handle: EventHandle = bcs::from_bytes(&event_handle_bytes).map_err(|e| {
//             Error::bad_request(format!(
//                 "field({}) type is not EventHandle struct, deserialize error: {}",
//                 field_name, e
//             ))
//         })?;
//         Ok(*event_handle.key())
//     }

//     pub fn find_resource(
//         &self,
//         struct_tag: &StructTag,
//     ) -> Result<Vec<(Identifier, MoveValue)>, Error> {
//         let account_state = self.account_state()?;
//         let (typ, data) = account_state
//             .get_resources()
//             .find(|(tag, _data)| tag == struct_tag)
//             .ok_or_else(|| self.resource_not_found(struct_tag))?;
//         Ok(self
//             .context
//             .move_converter()
//             .move_struct_fields(&typ, data)?)
//     }

//     fn account_state(&self) -> Result<AccountState, Error> {
//         let state = self
//             .context
//             .get_account_state(self.address.into(), self.ledger_version)?
//             .ok_or_else(|| self.account_not_found())?;
//         Ok(state)
//     }

//     fn account_not_found(&self) -> Error {
//         Error::not_found(
//             "account",
//             format!(
//                 "address({}) and ledger version({})",
//                 self.address, self.ledger_version,
//             ),
//             self.latest_ledger_info.version(),
//         )
//     }

//     fn resource_not_found(&self, struct_tag: &StructTag) -> Error {
//         Error::not_found(
//             "resource",
//             format!(
//                 "address({}), struct tag({}) and ledger version({})",
//                 self.address, struct_tag, self.ledger_version,
//             ),
//             self.latest_ledger_info.version(),
//         )
//     }

//     fn field_not_found(&self, struct_tag: &StructTag, field_name: &Identifier) -> Error {
//         Error::not_found(
//             "resource",
//             format!(
//                 "address({}), struct tag({}), field name({}) and ledger version({})",
//                 self.address, struct_tag, field_name, self.ledger_version,
//             ),
//             self.latest_ledger_info.version(),
//         )
//     }

//     #[tokio::test]
//     async fn test_get_account_resources_by_invalid_address_missing_0x_prefix() {
//         let context = new_test_context();
//         let invalid_addresses = vec!["1", "0xzz", "01"];
//         for invalid_address in &invalid_addresses {
//             let resp = context
//                 .expect_status_code(400)
//                 .get(&account_resources(invalid_address))
//                 .await;
//             assert_eq!(
//                 json!({
//                     "code": 400,
//                     "message": format!("invalid parameter account address: {}", invalid_address),
//                 }),
//                 resp
//             );
//         }
//     }

//     #[tokio::test]
//     async fn test_get_account_resources_by_valid_account_address() {
//         let context = new_test_context();
//         let addresses = vec![
//             "0xdd",
//             "000000000000000000000000000000dd",
//             "0x000000000000000000000000000000dd",
//         ];
//         for address in &addresses {
//             context.get(&account_resources(address)).await;
//         }
//     }

//     #[tokio::test]
//     async fn test_account_resources_response() {
//         let context = new_test_context();
//         let address = "0xdd";

//         let resp = context.get(&account_resources(address)).await;

//         let res = find_value(&resp, |v| {
//             v["type"]["name"] == "Balance" && v["type"]["generic_type_params"][0]["name"] == "XDX"
//         });
//         assert_json(
//             res,
//             json!({
//                 "type": {
//                     "type": "struct",
//                     "address": "0x1",
//                     "module": "DiemAccount",
//                     "name": "Balance",
//                     "generic_type_params": [
//                         {
//                             "type": "struct",
//                             "address": "0x1",
//                             "module": "XDX",
//                             "name": "XDX",
//                             "generic_type_params": []
//                         }
//                     ]
//                 },
//                 "value": {
//                     "coin": {
//                         "value": "0"
//                     }
//                 }
//             }),
//         );
//     }

//     #[tokio::test]
//     async fn test_account_modules() {
//         let context = new_test_context();
//         let address = "0x1";

//         let resp = context.get(&account_modules(address)).await;

//         let res = find_value(&resp, |v| v["abi"]["name"] == "BCS");
//         assert!(res["bytecode"].as_str().unwrap().starts_with("0x"));
//         assert_json(
//             res["abi"].clone(),
//             json!({
//                 "address": "0x1",
//                 "name": "BCS",
//                 "friends": [],
//                 "exposed_functions": [
//                     {
//                         "name": "to_bytes",
//                         "visibility": "public",
//                         "generic_type_params": [
//                             {
//                                 "constraints": []
//                             }
//                         ],
//                         "params": [
//                             {
//                                 "type": "reference",
//                                 "mutable": false,
//                                 "to": {
//                                     "type": "generic_type_param",
//                                     "index": 0
//                                 }
//                             }
//                         ],
//                         "return": [
//                             {
//                                 "type": "vector",
//                                 "items": {
//                                     "type": "u8"
//                                 }
//                             }
//                         ]
//                     }
//                 ],
//                 "structs": []
//             }),
//         );
//     }

//     #[tokio::test]
//     async fn test_get_module_with_script_functions() {
//         let context = new_test_context();
//         let address = "0x1";

//         let resp = context.get(&account_modules(address)).await;
//         let res = find_value(&resp, |v| v["abi"]["name"] == "PaymentScripts");
//         assert_json(
//             res["abi"].clone(),
//             json!({
//                 "address": "0x1",
//                 "name": "PaymentScripts",
//                 "friends": [],
//                 "exposed_functions": [
//                     {
//                         "name": "peer_to_peer_by_signers",
//                         "visibility": "script",
//                         "generic_type_params": [
//                             {
//                                 "constraints": []
//                             }
//                         ],
//                         "params": [
//                             {"type": "signer"},
//                             {"type": "signer"},
//                             {"type": "u64"},
//                             {
//                                 "type": "vector",
//                                 "items": {"type": "u8"}
//                             }
//                         ],
//                         "return": []
//                     },
//                     {
//                         "name": "peer_to_peer_with_metadata",
//                         "visibility": "script",
//                         "generic_type_params": [
//                             {
//                                 "constraints": []
//                             }
//                         ],
//                         "params": [
//                             {"type": "signer"},
//                             {"type": "address"},
//                             {"type": "u64"},
//                             {
//                                 "type": "vector",
//                                 "items": {"type": "u8"}
//                             },
//                             {
//                                 "type": "vector",
//                                 "items": {"type": "u8"}
//                             }
//                         ],
//                         "return": []
//                     }
//                 ],
//                 "structs": []
//             }),
//         );
//     }

//     #[tokio::test]
//     async fn test_get_module_diem_config() {
//         let context = new_test_context();
//         let address = "0x1";

//         let resp = context.get(&account_modules(address)).await;
//         let res = find_value(&resp, |v| v["abi"]["name"] == "DiemConfig");
//         assert_json(
//             res["abi"].clone(),
//             json!({
//                 "address": "0x1",
//                 "name": "DiemConfig",
//                 "friends": [
//                     {
//                         "address": "0x1",
//                         "name": "DiemConsensusConfig"
//                     },
//                     {
//                         "address": "0x1",
//                         "name": "DiemSystem"
//                     },
//                     {
//                         "address": "0x1",
//                         "name": "DiemTransactionPublishingOption"
//                     },
//                     {
//                         "address": "0x1",
//                         "name": "DiemVMConfig"
//                     },
//                     {
//                         "address": "0x1",
//                         "name": "DiemVersion"
//                     },
//                     {
//                         "address": "0x1",
//                         "name": "RegisteredCurrencies"
//                     }
//                 ],
//                 "exposed_functions": [
//                     {
//                         "name": "get",
//                         "visibility": "public",
//                         "generic_type_params": [
//                             {
//                                 "constraints": [
//                                     "copy",
//                                     "drop",
//                                     "store"
//                                 ]
//                             }
//                         ],
//                         "params": [],
//                         "return": [
//                             {
//                                 "type": "generic_type_param",
//                                 "index": 0
//                             }
//                         ]
//                     },
//                     {
//                         "name": "initialize",
//                         "visibility": "public",
//                         "generic_type_params": [],
//                         "params": [
//                             {
//                                 "type": "reference",
//                                 "mutable": false,
//                                 "to": {
//                                     "type": "signer"
//                                 }
//                             }
//                         ],
//                         "return": []
//                     },
//                     {
//                         "name": "publish_new_config",
//                         "visibility": "friend",
//                         "generic_type_params": [
//                             {
//                                 "constraints": [
//                                     "copy",
//                                     "drop",
//                                     "store"
//                                 ]
//                             }
//                         ],
//                         "params": [
//                             {
//                                 "type": "reference",
//                                 "mutable": false,
//                                 "to": {
//                                     "type": "signer"
//                                 }
//                             },
//                             {
//                                 "type": "generic_type_param",
//                                 "index": 0
//                             }
//                         ],
//                         "return": []
//                     },
//                     {
//                         "name": "publish_new_config_and_get_capability",
//                         "visibility": "friend",
//                         "generic_type_params": [
//                             {
//                                 "constraints": [
//                                     "copy",
//                                     "drop",
//                                     "store"
//                                 ]
//                             }
//                         ],
//                         "params": [
//                             {
//                                 "type": "reference",
//                                 "mutable": false,
//                                 "to": {
//                                     "type": "signer"
//                                 }
//                             },
//                             {
//                                 "type": "generic_type_param",
//                                 "index": 0
//                             }
//                         ],
//                         "return": [
//                             {
//                                 "type": "struct",
//                                 "address": "0x1",
//                                 "module": "DiemConfig",
//                                 "name": "ModifyConfigCapability",
//                                 "generic_type_params": [
//                                     {
//                                         "type": "generic_type_param",
//                                         "index": 0
//                                     }
//                                 ]
//                             }
//                         ]
//                     },
//                     {
//                         "name": "reconfigure",
//                         "visibility": "public",
//                         "generic_type_params": [],
//                         "params": [
//                             {
//                                 "type": "reference",
//                                 "mutable": false,
//                                 "to": {
//                                     "type": "signer"
//                                 }
//                             }
//                         ],
//                         "return": []
//                     },
//                     {
//                         "name": "set",
//                         "visibility": "friend",
//                         "generic_type_params": [
//                             {
//                                 "constraints": [
//                                     "copy",
//                                     "drop",
//                                     "store"
//                                 ]
//                             }
//                         ],
//                         "params": [
//                             {
//                                 "type": "reference",
//                                 "mutable": false,
//                                 "to": {
//                                     "type": "signer"
//                                 }
//                             },
//                             {
//                                 "type": "generic_type_param",
//                                 "index": 0
//                             }
//                         ],
//                         "return": []
//                     },
//                     {
//                         "name": "set_with_capability_and_reconfigure",
//                         "visibility": "friend",
//                         "generic_type_params": [
//                             {
//                                 "constraints": [
//                                     "copy",
//                                     "drop",
//                                     "store"
//                                 ]
//                             }
//                         ],
//                         "params": [
//                             {
//                                 "type": "reference",
//                                 "mutable": false,
//                                 "to": {
//                                     "type": "struct",
//                                     "address": "0x1",
//                                     "module": "DiemConfig",
//                                     "name": "ModifyConfigCapability",
//                                     "generic_type_params": [
//                                         {
//                                             "type": "generic_type_param",
//                                             "index": 0
//                                         }
//                                     ]
//                                 }
//                             },
//                             {
//                                 "type": "generic_type_param",
//                                 "index": 0
//                             }
//                         ],
//                         "return": []
//                     }
//                 ],
//                 "structs": [
//                     {
//                         "name": "Configuration",
//                         "is_native": false,
//                         "abilities": [
//                             "key"
//                         ],
//                         "generic_type_params": [],
//                         "fields": [
//                             {
//                                 "name": "epoch",
//                                 "type": {
//                                     "type": "u64"
//                                 }
//                             },
//                             {
//                                 "name": "last_reconfiguration_time",
//                                 "type": {
//                                     "type": "u64"
//                                 }
//                             },
//                             {
//                                 "name": "events",
//                                 "type": {
//                                     "type": "struct",
//                                     "address": "0x1",
//                                     "module": "Event",
//                                     "name": "EventHandle",
//                                     "generic_type_params": [
//                                         {
//                                             "type": "struct",
//                                             "address": "0x1",
//                                             "module": "DiemConfig",
//                                             "name": "NewEpochEvent",
//                                             "generic_type_params": []
//                                         }
//                                     ]
//                                 }
//                             }
//                         ]
//                     },
//                     {
//                         "name": "DiemConfig",
//                         "is_native": false,
//                         "abilities": [
//                             "store",
//                             "key"
//                         ],
//                         "generic_type_params": [
//                             {
//                                 "constraints": [
//                                     "copy",
//                                     "drop",
//                                     "store"
//                                 ],
//                                 "is_phantom": false
//                             }
//                         ],
//                         "fields": [
//                             {
//                                 "name": "payload",
//                                 "type": {
//                                     "type": "generic_type_param",
//                                     "index": 0
//                                 }
//                             }
//                         ]
//                     },
//                     {
//                         "name": "DisableReconfiguration",
//                         "is_native": false,
//                         "abilities": [
//                             "key"
//                         ],
//                         "generic_type_params": [],
//                         "fields": [
//                             {
//                                 "name": "dummy_field",
//                                 "type": {
//                                     "type": "bool"
//                                 }
//                             }
//                         ]
//                     },
//                     {
//                         "name": "ModifyConfigCapability",
//                         "is_native": false,
//                         "abilities": [
//                             "store",
//                             "key"
//                         ],
//                         "generic_type_params": [
//                             {
//                                 "constraints": [],
//                                 "is_phantom": true
//                             }
//                         ],
//                         "fields": [
//                             {
//                                 "name": "dummy_field",
//                                 "type": {
//                                     "type": "bool"
//                                 }
//                             }
//                         ]
//                     },
//                     {
//                         "name": "NewEpochEvent",
//                         "is_native": false,
//                         "abilities": [
//                             "drop",
//                             "store"
//                         ],
//                         "generic_type_params": [],
//                         "fields": [
//                             {
//                                 "name": "epoch",
//                                 "type": {
//                                     "type": "u64"
//                                 }
//                             }
//                         ]
//                     }
//                 ]
//             }),
//         );
//     }

//     #[tokio::test]
//     async fn test_account_modules_structs() {
//         let context = new_test_context();
//         let address = "0x1";

//         let resp = context.get(&account_modules(address)).await;

//         let diem_account_module = find_value(&resp, |v| v["abi"]["name"] == "DiemAccount");
//         let balance_struct = find_value(&diem_account_module["abi"]["structs"], |v| {
//             v["name"] == "Balance"
//         });
//         assert_json(
//             balance_struct,
//             json!({
//                 "name": "Balance",
//                 "is_native": false,
//                 "abilities": [
//                     "key"
//                 ],
//                 "generic_type_params": [
//                     {
//                         "constraints": [],
//                         "is_phantom": true
//                     }
//                 ],
//                 "fields": [
//                     {
//                         "name": "coin",
//                         "type": {
//                             "type": "struct",
//                             "address": "0x1",
//                             "module": "Diem",
//                             "name": "Diem",
//                             "generic_type_params": [
//                                 {
//                                     "type": "generic_type_param",
//                                     "index": 0
//                                 }
//                             ]
//                         }
//                     }
//                 ]
//             }),
//         );

//         let diem_module = find_value(&resp, |f| f["abi"]["name"] == "Diem");
//         let diem_struct = find_value(&diem_module["abi"]["structs"], |f| f["name"] == "Diem");
//         assert_json(
//             diem_struct,
//             json!({
//                 "name": "Diem",
//                 "is_native": false,
//                 "abilities": [
//                     "store"
//                 ],
//                 "generic_type_params": [
//                     {
//                         "constraints": [],
//                         "is_phantom": true
//                     }
//                 ],
//                 "fields": [
//                     {
//                         "name": "value",
//                         "type": {
//                             "type": "u64"
//                         }
//                     }
//                 ]
//             }),
//         );
//     }

//     #[tokio::test]
//     async fn test_get_account_resources_by_ledger_version() {
//         let mut context = new_test_context();
//         let account = context.gen_account();
//         let txn = context.create_parent_vasp(&account);
//         context.commit_block(&vec![txn.clone()]);

//         let ledger_version_1_resources = context
//             .get(&account_resources(
//                 &context.tc_account().address().to_hex_literal(),
//             ))
//             .await;
//         let tc_account = find_value(&ledger_version_1_resources, |f| {
//             f["type"]["name"] == "DiemAccount"
//         });
//         assert_eq!(tc_account["value"]["sequence_number"], "1");

//         let ledger_version_0_resources = context
//             .get(&account_resources_with_ledger_version(
//                 &context.tc_account().address().to_hex_literal(),
//                 0,
//             ))
//             .await;
//         let tc_account = find_value(&ledger_version_0_resources, |f| {
//             f["type"]["name"] == "DiemAccount"
//         });
//         assert_eq!(tc_account["value"]["sequence_number"], "0");
//     }

//     #[tokio::test]
//     async fn test_get_account_resources_by_ledger_version_is_too_large() {
//         let context = new_test_context();
//         let resp = context
//             .expect_status_code(404)
//             .get(&account_resources_with_ledger_version(
//                 &context.tc_account().address().to_hex_literal(),
//                 1000000000000000000,
//             ))
//             .await;
//         assert_json(
//             resp,
//             json!({
//                 "code": 404,
//                 "message": "ledger not found by version(1000000000000000000)",
//                 "diem_ledger_version": "0"
//             }),
//         );
//     }

//     #[tokio::test]
//     async fn test_get_account_resources_by_invalid_ledger_version() {
//         let context = new_test_context();
//         let resp = context
//             .expect_status_code(400)
//             .get(&account_resources_with_ledger_version(
//                 &context.tc_account().address().to_hex_literal(),
//                 -1,
//             ))
//             .await;
//         assert_json(
//             resp,
//             json!({
//                 "code": 400,
//                 "message": "invalid parameter ledger version: -1"
//             }),
//         );
//     }

//     #[tokio::test]
//     async fn test_get_account_modules_by_ledger_version() {
//         let context = new_test_context();
//         let code = "a11ceb0b0300000006010002030205050703070a0c0816100c260900000001000100000102084d794d6f64756c650269640000000000000000000000000b1e55ed00010000000231010200";
//         let mut tc_account = context.tc_account();
//         let txn = tc_account.sign_with_transaction_builder(
//             context
//                 .transaction_factory()
//                 .module(hex::decode(code).unwrap()),
//         );
//         context.commit_block(&vec![txn.clone()]);

//         let modules = context
//             .get(&account_modules(
//                 &context.tc_account().address().to_hex_literal(),
//             ))
//             .await;
//         assert_ne!(modules, json!([]));

//         let modules = context
//             .get(&account_modules_with_ledger_version(
//                 &context.tc_account().address().to_hex_literal(),
//                 0,
//             ))
//             .await;
//         assert_eq!(modules, json!([]));
//     }

//     fn account_resources(address: &str) -> String {
//         format!("/accounts/{}/resources", address)
//     }

//     fn account_resources_with_ledger_version(address: &str, ledger_version: i128) -> String {
//         format!("/ledger/{}{}", ledger_version, get_account_resources(address))
//     }

//     fn account_modules(address: &str) -> String {
//         format!("/accounts/{}/modules", address)
//     }

//     fn account_modules_with_ledger_version(address: &str, ledger_version: i128) -> String {
//         format!("/ledger/{}{}", ledger_version, get_account_modules(address))
//     }
// }
