//! Zahlen, wie sie die Oberfläche zeigt: mit Dezimalkomma und, ab fünf
//! Stellen vor dem Komma, mit Punkten zwischen den Tausendern ("3853 m/s",
//! "30.000 kg", "220,5 km"). Eine Stelle für alle Anzeigen, damit nicht
//! Punkt und Komma nebeneinander stehen.

use std::fmt;

/// Eine Zahl für die Anzeige. Die Genauigkeit kommt wie gewohnt aus dem
/// Format (`{:.1}`), ebenso Breite und Ausrichtung; ohne Angabe: ganze Zahl.
pub struct De(pub f64);

impl fmt::Display for De {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = format!("{:.*}", f.precision().unwrap_or(0), self.0);
        let (number, fraction) = plain.split_once('.').unwrap_or((&plain, ""));
        let (sign, digits) = match number.strip_prefix('-') {
            Some(digits) => ("-", digits),
            None => ("", number),
        };
        let mut text = String::from(sign);
        for (i, digit) in digits.chars().enumerate() {
            if digits.len() >= 5 && i > 0 && (digits.len() - i).is_multiple_of(3) {
                text.push('.');
            }
            text.push(digit);
        }
        if !fraction.is_empty() {
            text.push(',');
            text.push_str(fraction);
        }
        // Breite und Ausrichtung von Hand: `pad` schnitte den Text auf die
        // Genauigkeit zu.
        let fill = f.width().unwrap_or(0).saturating_sub(text.chars().count());
        let (left, right) = match f.align() {
            Some(fmt::Alignment::Left) => (0, fill),
            Some(fmt::Alignment::Center) => (fill / 2, fill - fill / 2),
            _ => (fill, 0),
        };
        let pad = |n: usize| f.fill().to_string().repeat(n);
        write!(f, "{}{text}{}", pad(left), pad(right))
    }
}

#[cfg(test)]
mod tests {
    use super::De;

    #[test]
    fn german_numbers() {
        assert_eq!(format!("{}", De(3853.4)), "3853");
        assert_eq!(format!("{}", De(30_000.0)), "30.000");
        assert_eq!(format!("{:.1}", De(1_234_567.25)), "1.234.567,2");
        assert_eq!(format!("{:.1}", De(220.46)), "220,5");
        assert_eq!(format!("{:.2}", De(-0.5)), "-0,50");
        assert_eq!(format!("{}", De(-12_345.0)), "-12.345");
        assert_eq!(
            format!("{:>7.1}|{:<6}|", De(2.0), De(15.0)),
            "    2,0|15    |"
        );
    }
}
