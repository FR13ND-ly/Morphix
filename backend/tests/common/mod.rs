#![allow(dead_code)]

use serde_json::{Value, json};

fn plain(type_id: u8) -> Value {
    json!({"type_id": type_id, "properties": {}})
}

pub fn moving_agents() -> Value {
    json!({
        "automaton": {
            "name": "Movement test",
            "object_types": [
                {"id":0,"name":"floor","properties":{}},
                {"id":1,"name":"walker","properties":{
                    "step":{"default_value":1,"min_value":-1,"max_value":1,"integer_only":true}
                }}
            ],
            "neighborhood":{"offsets":[[0,-1],[0,1]]},
            "boundary":{"kind":"wrap"},
            "rules":[{
                "target":"agent","condition":{"kind":"type_is","args":1},"probability":1.0,
                "actions":[{"kind":"move","row_offset":{"kind":"constant","args":0},"col_offset":{"kind":"property","args":"step"}}]
            }]
        },
        "initial_world":{
            "cells":[vec![plain(0);5]],
            "agents":[
                {"id":7,"position":{"row":0,"col":0},"state":{"type_id":1,"properties":{"step":1}}},
                {"id":8,"position":{"row":0,"col":4},"state":{"type_id":1,"properties":{"step":-1}}}
            ]
        },
        "generations":3,"seed":42,"save_history":true
    })
}

pub fn free_movement() -> Value {
    let mut request = moving_agents();
    request["automaton"]["object_types"][1]["properties"] = json!({});
    request["automaton"]["neighborhood"]["offsets"] = json!([[0, 1]]);
    request["automaton"]["rules"][0]["actions"] = json!([
        {"kind":"move_to_free_neighbor","selection":"first","cell_type":0}
    ]);
    request["initial_world"]["cells"] = json!([vec![plain(0); 10]]);
    request["initial_world"]["agents"] = json!(
        [1, 2, 4, 8]
            .into_iter()
            .enumerate()
            .map(|(index, col)| {
                json!({"id":index+1,"position":{"row":0,"col":col},"state":plain(1)})
            })
            .collect::<Vec<_>>()
    );
    request
}

pub fn captures() -> Value {
    json!({
        "automaton":{
            "name":"Capture test",
            "object_types":[
                {"id":0,"name":"ground","properties":{}},
                {"id":1,"name":"prey","properties":{}},
                {"id":2,"name":"predator","properties":{"energy":{"default_value":10,"min_value":0,"max_value":100,"integer_only":true}}}
            ],
            "neighborhood":{"offsets":[[0,-1],[0,1]]},"boundary":{"kind":"wrap"},
            "rules":[
                {"target":"agent","condition":{"kind":"all","args":[
                    {"kind":"type_is","args":2},
                    {"kind":"compare","args":{"left":{"kind":"property","args":"energy"},"operator":{"kind":"less_or_equal"},"right":{"kind":"constant","args":0}}}
                ]},"probability":1,"actions":[{"kind":"remove_agent"}]},
                {"target":"agent","condition":{"kind":"all","args":[
                    {"kind":"type_is","args":2},
                    {"kind":"compare","args":{"left":{"kind":"count_neighbor_agents","args":{"type_id":1}},"operator":{"kind":"greater"},"right":{"kind":"constant","args":0}}}
                ]},"probability":1,"actions":[
                    {"kind":"remove_neighbor_agent","type_id":1,"selection":"random"},
                    {"kind":"move_to_removed_neighbor"},
                    {"kind":"set_property","property":"energy","value":{"kind":"add","args":[{"kind":"property","args":"energy"},{"kind":"constant","args":10}]}}
                ]},
                {"target":"agent","condition":{"kind":"type_is","args":2},"probability":1,"actions":[
                    {"kind":"set_property","property":"energy","value":{"kind":"subtract","args":[{"kind":"property","args":"energy"},{"kind":"constant","args":1}]}},
                    {"kind":"move_to_free_neighbor","selection":"random","cell_type":0}
                ]},
                {"target":"agent","condition":{"kind":"type_is","args":1},"probability":1,"actions":[{"kind":"move_to_free_neighbor","selection":"random","cell_type":0}]}
            ]
        },
        "initial_world":{"cells":[vec![plain(0);5]],"agents":[
            {"id":1,"position":{"row":0,"col":1},"state":plain(1)},
            {"id":2,"position":{"row":0,"col":0},"state":{"type_id":2,"properties":{"energy":10}}},
            {"id":3,"position":{"row":0,"col":4},"state":plain(1)}
        ]},
        "generations":5,"seed":42,"save_history":true
    })
}

pub fn directional_cells() -> Value {
    let cells: Vec<_> = (0..11)
        .map(|index| plain(if index == 5 { 1 } else { 0 }))
        .collect();
    json!({
        "automaton":{
            "name":"Directional cells test",
            "object_types":[{"id":0,"name":"dead","properties":{}},{"id":1,"name":"alive","properties":{}}],
            "neighborhood":{"offsets":[[0,-1],[0,1]]},"boundary":{"kind":"fixed","state":plain(0)},
            "rules":[
                {"target":"cell","condition":{"kind":"any","args":[
                    {"kind":"all","args":[
                        {"kind":"neighbor_type_is","args":{"offset":[0,-1],"type_id":1}},
                        {"kind":"type_is","args":0},
                        {"kind":"neighbor_type_is","args":{"offset":[0,1],"type_id":0}}
                    ]},
                    {"kind":"all","args":[
                        {"kind":"neighbor_type_is","args":{"offset":[0,-1],"type_id":0}},
                        {"kind":"any","args":[{"kind":"type_is","args":1},{"kind":"neighbor_type_is","args":{"offset":[0,1],"type_id":1}}]}
                    ]}
                ]},"probability":1,"actions":[{"kind":"set_type","type_id":1}]},
                {"target":"cell","condition":{"kind":"always"},"probability":1,"actions":[{"kind":"set_type","type_id":0}]}
            ]
        },
        "initial_world":{"cells":[cells],"agents":[]},"generations":4,"seed":42,"save_history":true
    })
}

pub fn numeric_transfers() -> Value {
    let offsets = [[-1, 0], [1, 0], [0, -1], [0, 1]];
    let mut actions = vec![json!({"kind":"set_property","property":"grains","value":{
        "kind":"subtract","args":[{"kind":"property","args":"grains"},{"kind":"constant","args":4}]
    }})];
    for offset in offsets {
        actions.push(json!({"kind":"add_neighbor_property","offset":offset,"property":"grains","value":{"kind":"constant","args":1}}));
    }
    let empty = json!({"type_id":0,"properties":{"grains":0}});
    let mut cells = vec![vec![empty.clone(); 3]; 3];
    cells[1][1]["properties"]["grains"] = json!(8);
    json!({
        "automaton":{
            "name":"Numeric transfers test",
            "object_types":[{"id":0,"name":"pile","properties":{"grains":{"default_value":0,"min_value":0,"max_value":1000000,"integer_only":true}}}],
            "neighborhood":{"offsets":offsets},"boundary":{"kind":"fixed","state":empty},
            "rules":[{"target":"cell","condition":{"kind":"compare","args":{
                "left":{"kind":"property","args":"grains"},"operator":{"kind":"greater_or_equal"},"right":{"kind":"constant","args":4}
            }},"probability":1,"actions":actions}]
        },
        "initial_world":{"cells":cells,"agents":[]},"generations":6,"seed":42,"save_history":true
    })
}
