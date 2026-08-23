//! 表达式解析与求值
//!
//! 使用 meval 库解析字符串表达式，支持变量 x、常量 pi/e 和自定义参数。

use crate::types::Parameter;
use meval::{Context, Expr};

/// 编译后的表达式（不可序列化，运行时缓存）
pub struct CompiledExpr {
    expr: Expr,
    source: String,
}

impl CompiledExpr {
    /// 解析表达式字符串
    ///
    /// # 错误
    /// 表达式语法错误时返回 `Err`，包含人类可读的错误信息。
    pub fn parse(expression: &str) -> anyhow::Result<Self> {
        let expr: Expr = expression
            .parse()
            .map_err(|e| anyhow::anyhow!("表达式语法错误: {}", format_meval_error(&e)))?;
        Ok(Self {
            expr,
            source: expression.to_string(),
        })
    }

    /// 在指定 x 值处求值（带参数上下文）
    ///
    /// 返回 `None` 表示求值失败（如除零、未定义函数等）。
    pub fn evaluate(&self, params: &[Parameter], x: f64) -> Option<f64> {
        let mut ctx = Context::default(); // 包含 pi, e
        for p in params {
            ctx.var(&p.name, p.value);
        }
        ctx.var("x", x);
        self.expr.eval_with_context(ctx).ok()
    }

    /// 获取原始表达式字符串
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// 将 meval 错误格式化为用户友好的中文提示
fn format_meval_error(e: &meval::Error) -> String {
    format!("{e:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parse() {
        let expr = CompiledExpr::parse("sin(x)").unwrap();
        let y = expr.evaluate(&[], 0.0).unwrap();
        assert!((y - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_with_params() {
        let expr = CompiledExpr::parse("a * x + b").unwrap();
        let params = vec![
            Parameter::new("a", 2.0, 0.0, 10.0),
            Parameter::new("b", 1.0, 0.0, 10.0),
        ];
        let y = expr.evaluate(&params, 3.0).unwrap();
        assert!((y - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_constants() {
        let expr = CompiledExpr::parse("sin(pi / 2)").unwrap();
        let y = expr.evaluate(&[], 0.0).unwrap();
        assert!((y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_syntax_error() {
        assert!(CompiledExpr::parse("sin(x").is_err());
    }

    #[test]
    fn test_sinc() {
        let expr = CompiledExpr::parse("sin(x) / x").unwrap();
        // x=0 时 sin(0)/0 是 NaN，但 meval 可能返回 inf 或 nan
        let y = expr.evaluate(&[], 1.0).unwrap();
        assert!((y - 0.8414709848).abs() < 1e-6);
    }
}
