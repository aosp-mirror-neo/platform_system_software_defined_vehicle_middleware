// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::anyhow;
use dice_policy::{
    Constraint, DicePolicy, NodeConstraints, EXACT_MATCH_CONSTRAINT, GREATER_OR_EQUAL_CONSTRAINT,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct DicePolicyJson(Vec<NodeConstraintsJson>);

#[derive(Debug, Deserialize)]
pub enum DicePolicyValue {
    Bool(bool),
    Int(i64),
    Text(String),
    #[serde(with = "base64_serde")]
    Bytes(Vec<u8>),
}

#[derive(Debug, Deserialize)]
pub struct NodeConstraintsJson(Vec<ConstraintJson>);

#[derive(Debug, Deserialize)]
pub enum ConstraintJson {
    ExactMatch { key_spec: Vec<i64>, value: DicePolicyValue },
    Ge { key_spec: Vec<i64>, value: i64 },
}

mod base64_serde {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde::Deserialize;
    use serde::Deserializer;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        URL_SAFE_NO_PAD.decode(base64.as_bytes()).map_err(serde::de::Error::custom)
    }
}

fn json_to_value(json: DicePolicyValue) -> ciborium::Value {
    match json {
        DicePolicyValue::Bool(b) => ciborium::Value::from(b),
        DicePolicyValue::Int(i) => ciborium::Value::from(i),
        DicePolicyValue::Text(s) => ciborium::Value::from(s),
        DicePolicyValue::Bytes(v) => ciborium::Value::from(v),
    }
}

fn json_to_constraint(json: ConstraintJson) -> anyhow::Result<Constraint> {
    match json {
        ConstraintJson::ExactMatch { key_spec, value } => {
            Ok(Constraint::new(EXACT_MATCH_CONSTRAINT, key_spec, json_to_value(value))
                .map_err(|e| anyhow!("Exact Match constraint creation failed: {:?}", e))?)
        }

        ConstraintJson::Ge { key_spec, value } => {
            Ok(Constraint::new(GREATER_OR_EQUAL_CONSTRAINT, key_spec, ciborium::Value::from(value))
                .map_err(|e| anyhow!("GE constraint creation failed: {:?}", e))?)
        }
    }
}

fn json_to_node_constraints(json: NodeConstraintsJson) -> anyhow::Result<NodeConstraints> {
    let result = NodeConstraints(
        json.0
            .into_iter()
            .map(json_to_constraint)
            .collect::<Result<Vec<Constraint>, _>>()?
            .into_boxed_slice(),
    );

    Ok(result)
}

pub fn json_to_dice_policy(json: DicePolicyJson) -> anyhow::Result<DicePolicy> {
    let node_cons_list: Vec<NodeConstraints> =
        json.0.into_iter().map(json_to_node_constraints).collect::<Result<_, _>>()?;

    Ok(DicePolicy { version: 1, node_constraints_list: node_cons_list.into_boxed_slice() })
}
