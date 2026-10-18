use crate::models::{
    automaton::AutomatonDefinition,
    expression::{Expression, Operator},
    grid::Grid,
};

use super::neighborhood::read;

#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub grid: &'a Grid,
    pub automaton: &'a AutomatonDefinition,
    pub row: i64,
    pub col: i64,
}

impl Context<'_> {
    fn shifted(&self, dy: i32, dx: i32) -> Self {
        Self {
            row: self.row + i64::from(dy),
            col: self.col + i64::from(dx),
            ..*self
        }
    }

    pub fn evaluate(
        &self,
        expression: &Expression,
        budget: &mut u64,
    ) -> Result<i64, &'static str> {
        if *budget == 0 {
            return Err("Limita de calcul a fost depășită");
        }

        *budget -= 1;

        match expression {
            Expression::Constant(value) => Ok(*value),

            Expression::Value => Ok(read(
                self.grid,
                self.automaton.boundary,
                self.row,
                self.col,
            ).value),

            Expression::CellType => Ok(read(
                self.grid,
                self.automaton.boundary,
                self.row,
                self.col,
            ).cell_type),

            Expression::Binary { operator, left, right } => {
                let a = self.evaluate(left, budget)?;

                if *operator == Operator::And && a == 0 {
                    return Ok(0);
                }

                if *operator == Operator::Or && a != 0 {
                    return Ok(1);
                }

                let b = self.evaluate(right, budget)?;

                let result = match operator {
                    Operator::Add => a.checked_add(b),
                    Operator::Subtract => a.checked_sub(b),
                    Operator::Multiply => a.checked_mul(b),
                    Operator::Divide => a.checked_div(b),
                    Operator::Equal => Some(i64::from(a == b)),
                    Operator::NotEqual => Some(i64::from(a != b)),
                    Operator::Less => Some(i64::from(a < b)),
                    Operator::LessOrEqual => Some(i64::from(a <= b)),
                    Operator::Greater => Some(i64::from(a > b)),
                    Operator::GreaterOrEqual => Some(i64::from(a >= b)),
                    Operator::And => Some(i64::from(a != 0 && b != 0)),
                    Operator::Or => Some(i64::from(a != 0 || b != 0)),
                };

                result.ok_or("Overflow sau împărțire la zero")
            }

            Expression::If { condition, then, otherwise } => {
                if self.evaluate(condition, budget)? != 0 {
                    self.evaluate(then, budget)
                } else {
                    self.evaluate(otherwise, budget)
                }
            }

            Expression::CountNeighbors(predicate) => {
                let mut count = 0;

                for &(dy, dx) in &self.automaton.neighborhood {
                    if self.shifted(dy, dx).evaluate(predicate, budget)? != 0 {
                        count += 1;
                    }
                }

                Ok(count)
            }

            Expression::SumNeighbors(expression) => {
                let mut sum = 0_i64;

                for &(dy, dx) in &self.automaton.neighborhood {
                    let value = self
                        .shifted(dy, dx)
                        .evaluate(expression, budget)?;

                    sum = sum.checked_add(value).ok_or("Overflow la însumare")?;
                }

                Ok(sum)
            }

            Expression::At { dy, dx, expression } => {
                self.shifted(*dy, *dx).evaluate(expression, budget)
            }
        }
    }
}