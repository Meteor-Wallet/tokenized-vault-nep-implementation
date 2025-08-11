#[derive(Clone, Copy)]
pub enum Rounding {
    Down,
    Up,
}

pub fn mul_div(x: u128, y: u128, denominator: u128, rounding: Rounding) -> u128 {
    use crate::contract_standards::U256;

    let numerator = U256::from(x) * U256::from(y);
    let denominator = U256::from(denominator);
    let result = numerator / denominator;
    let remainder = numerator % denominator;

    match rounding {
        Rounding::Down => result.as_u128(),
        Rounding::Up => {
            if remainder > U256::zero() {
                result.as_u128() + 1
            } else {
                result.as_u128()
            }
        }
    }
}
