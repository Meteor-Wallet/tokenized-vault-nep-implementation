#[derive(Clone, Copy)]
pub enum Rounding {
    Down,
    Up,
}

pub fn mul_div(x: u128, y: u128, denominator: u128, rounding: Rounding) -> u128 {
    let numerator = x.checked_mul(y).expect("mul overflow");
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;

    match rounding {
        Rounding::Down => quotient,
        Rounding::Up => {
            if remainder > 0 {
                quotient + 1
            } else {
                quotient
            }
        }
    }
}
