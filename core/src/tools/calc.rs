//! Tool `calc`: evalúa expresiones aritméticas (+ - * / y paréntesis).

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolResult};

/// Evalúa expresiones aritméticas simples sin dependencias externas.
pub struct CalcTool;

#[async_trait]
impl Tool for CalcTool {
    fn name(&self) -> &str {
        "calc"
    }

    fn description(&self) -> &str {
        "Evalúa una expresión aritmética (suma, resta, multiplicación, división y paréntesis) y devuelve el resultado numérico."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "expr": {
                    "type": "string",
                    "description": "La expresión a evaluar, p.ej. \"23 * 19\" o \"(2 + 3) / 5\"."
                }
            },
            "required": ["expr"]
        })
    }

    async fn execute(&self, args: &Value) -> ToolResult {
        let expr = args
            .get("expr")
            .and_then(Value::as_str)
            .ok_or_else(|| "falta el argumento 'expr'".to_string())?;

        let value = Parser::new(expr).evaluate()?;
        Ok(format_number(value))
    }
}

/// Formatea sin decimales innecesarios: `437`, `3.5`, `-3`.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        let s = format!("{n}");
        s
    }
}

/// Parser recursivo-descendente: expr = term (('+'|'-') term)*,
/// term = factor (('*'|'/') factor)*, factor = number | '(' expr ')' | '-' factor.
struct Parser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { chars: input.chars().peekable() }
    }

    fn evaluate(&mut self) -> Result<f64, String> {
        let value = self.expr()?;
        self.skip_ws();
        if self.chars.peek().is_some() {
            return Err("expresión inválida: caracteres sobrantes".into());
        }
        Ok(value)
    }

    fn skip_ws(&mut self) {
        while matches!(self.chars.peek(), Some(c) if c.is_whitespace()) {
            self.chars.next();
        }
    }

    fn expr(&mut self) -> Result<f64, String> {
        let mut acc = self.term()?;
        loop {
            self.skip_ws();
            match self.chars.peek() {
                Some('+') => {
                    self.chars.next();
                    acc += self.term()?;
                }
                Some('-') => {
                    self.chars.next();
                    acc -= self.term()?;
                }
                _ => break,
            }
        }
        Ok(acc)
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut acc = self.factor()?;
        loop {
            self.skip_ws();
            match self.chars.peek() {
                Some('*') => {
                    self.chars.next();
                    acc *= self.factor()?;
                }
                Some('/') => {
                    self.chars.next();
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return Err("división por cero".into());
                    }
                    acc /= divisor;
                }
                _ => break,
            }
        }
        Ok(acc)
    }

    fn factor(&mut self) -> Result<f64, String> {
        self.skip_ws();
        match self.chars.peek() {
            Some('-') => {
                self.chars.next();
                Ok(-self.factor()?)
            }
            Some('(') => {
                self.chars.next();
                let value = self.expr()?;
                self.skip_ws();
                match self.chars.next() {
                    Some(')') => Ok(value),
                    _ => Err("falta el paréntesis de cierre".into()),
                }
            }
            Some(c) if c.is_ascii_digit() || *c == '.' => self.number(),
            _ => Err("se esperaba un número".into()),
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        let mut s = String::new();
        while let Some(c) = self.chars.peek() {
            if c.is_ascii_digit() || *c == '.' {
                s.push(*c);
                self.chars.next();
            } else {
                break;
            }
        }
        s.parse::<f64>().map_err(|_| format!("número inválido: '{s}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn eval(expr: &str) -> ToolResult {
        CalcTool.execute(&json!({ "expr": expr })).await
    }

    #[tokio::test]
    async fn multiplies() {
        assert_eq!(eval("23 * 19").await.unwrap(), "437");
    }

    #[tokio::test]
    async fn respects_precedence_and_parentheses() {
        assert_eq!(eval("2 + 3 * 4").await.unwrap(), "14");
        assert_eq!(eval("(2 + 3) * 4").await.unwrap(), "20");
    }

    #[tokio::test]
    async fn handles_decimals_and_division() {
        assert_eq!(eval("7 / 2").await.unwrap(), "3.5");
    }

    #[tokio::test]
    async fn unary_minus() {
        assert_eq!(eval("-5 + 2").await.unwrap(), "-3");
    }

    #[tokio::test]
    async fn division_by_zero_is_error_not_panic() {
        assert!(eval("1 / 0").await.is_err());
    }

    #[tokio::test]
    async fn invalid_expression_is_error() {
        assert!(eval("2 +").await.is_err());
        assert!(eval("abc").await.is_err());
    }

    #[tokio::test]
    async fn missing_expr_argument_is_error() {
        assert!(CalcTool.execute(&json!({})).await.is_err());
    }

    #[test]
    fn spec_has_expected_name_and_param() {
        let spec = CalcTool.spec();
        assert_eq!(spec.function.name, "calc");
        assert_eq!(spec.function.parameters["properties"]["expr"]["type"], "string");
        assert!(!CalcTool.requires_permission());
    }
}
