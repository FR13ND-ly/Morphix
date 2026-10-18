use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    And,
    Or,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", content = "args", rename_all = "snake_case")]
pub enum Expression {
    Constant(i64),
    Value,
    CellType,

    Binary {
        operator: Operator,
        left: Box<Expression>,
        right: Box<Expression>,
    },

    If {
        condition: Box<Expression>,
        then: Box<Expression>,
        otherwise: Box<Expression>,
    },

    CountNeighbors(Box<Expression>),
    SumNeighbors(Box<Expression>),

    At {
        dy: i32,
        dx: i32,
        expression: Box<Expression>,
    },
}

impl Expression {
    pub fn binary(
        operator: Operator,
        left: Expression,
        right: Expression,
    ) -> Self {
        Self::Binary {
            operator,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub fn if_else(
        condition: Expression,
        then: Expression,
        otherwise: Expression,
    ) -> Self {
        Self::If {
            condition: Box::new(condition),
            then: Box::new(then),
            otherwise: Box::new(otherwise),
        }
    }
}