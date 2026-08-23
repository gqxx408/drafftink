//! 手写递归下降表达式解析器（零外部依赖）。
//!
//! 供「📈 函数绘图」使用：把 `y = 2x + 1` / `sin(x)` 这类表达式解析成 [`Expr`]
//! 抽象语法树，然后在坐标系上采样绘制曲线。
//!
//! 支持的语法：
//! - 数字字面量：`1` `2.5` `.5`
//! - 变量：`x`（大小写均可，`X` 归一化为 `x`）
//! - 四则运算：`+ - * /`（`*` 可省略，如 `2x` = `2*x`）
//! - 幂运算 `^`（右结合，如 `x^2` = `x**2`）
//! - 一元正负：`-x` `+3`
//! - 括号：`(expr)`
//! - 函数：`sin(x)` `cos(x)` `tan(x)`
//! - 容错：忽略 `y =` / `Y =` / `f(x) =` 前缀与空白
//!
//! 不支持的语法返回带位置的清晰错误（`Result<_, String>`），**绝不 panic**。

/// 表达式抽象语法树。
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const(f32),
    /// 自变量 `x`。
    Var,
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    /// 幂运算 `a ^ b`（右结合）。
    Pow(Box<Expr>, Box<Expr>),
    Sin(Box<Expr>),
    Cos(Box<Expr>),
    Tan(Box<Expr>),
}

impl Expr {
    /// 对给定的 `x` 求值。除零返回 `f32::NAN`（绘图时跳过该采样点）。
    pub fn eval(&self, x: f32) -> f32 {
        match self {
            Expr::Const(c) => *c,
            Expr::Var => x,
            Expr::Add(a, b) => a.eval(x) + b.eval(x),
            Expr::Sub(a, b) => a.eval(x) - b.eval(x),
            Expr::Mul(a, b) => a.eval(x) * b.eval(x),
            Expr::Div(a, b) => {
                let d = b.eval(x);
                if d == 0.0 {
                    f32::NAN
                } else {
                    a.eval(x) / d
                }
            }
            Expr::Pow(a, b) => a.eval(x).powf(b.eval(x)),
            Expr::Sin(a) => a.eval(x).sin(),
            Expr::Cos(a) => a.eval(x).cos(),
            Expr::Tan(a) => a.eval(x).tan(),
        }
    }
}

// ── 递归下降解析器 ──────────────────────────────────────────────────────────

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn err(&self, msg: &str) -> String {
        let shown: String = self.chars.iter().skip(self.pos).take(12).collect();
        format!("{msg}（位置 {}，附近: \"{shown}\"）", self.pos + 1)
    }

    /// expr := term (('+'|'-') term)*
    fn parse_expr(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    let rhs = self.parse_term()?;
                    lhs = Expr::Add(Box::new(lhs), Box::new(rhs));
                }
                Some('-') => {
                    self.pos += 1;
                    let rhs = self.parse_term()?;
                    lhs = Expr::Sub(Box::new(lhs), Box::new(rhs));
                }
                _ => return Ok(lhs),
            }
        }
    }

    /// term := factor (('*'|'/'|隐式乘) factor)*
    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_factor()?;
        loop {
            let token_end = self.pos; // 上一 token 消费到的位置（skip_ws 之前）。
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    let rhs = self.parse_factor()?;
                    lhs = Expr::Mul(Box::new(lhs), Box::new(rhs));
                }
                Some('/') => {
                    self.pos += 1;
                    let rhs = self.parse_factor()?;
                    lhs = Expr::Div(Box::new(lhs), Box::new(rhs));
                }
                // 隐式乘法：`2x`、`3(x+1)`、`(x)(x)`。仅当**无空白紧邻**时生效
                // （`1 2` 是两个数字，应报「多余内容」而非 1*2）。
                Some(c)
                    if self.pos == token_end
                        && (c.is_ascii_digit()
                            || c == 'x'
                            || c == 'X'
                            || c == '('
                            || c.is_alphabetic()) =>
                {
                    let rhs = self.parse_factor()?;
                    lhs = Expr::Mul(Box::new(lhs), Box::new(rhs));
                }
                _ => return Ok(lhs),
            }
        }
    }

    /// factor := ('-'|'+') factor | power
    fn parse_factor(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        match self.peek() {
            Some('-') => {
                self.pos += 1;
                Ok(Expr::Sub(
                    Box::new(Expr::Const(0.0)),
                    Box::new(self.parse_factor()?),
                ))
            }
            Some('+') => {
                self.pos += 1;
                self.parse_factor()
            }
            _ => self.parse_power(),
        }
    }

    /// power := primary ('^' factor)?
    ///
    /// 右结合、指数允许一元（`-x^2` = `-(x^2)`）。因 `^` 优先级高于一元负号，
    /// 故指数侧递归用 `parse_factor`（可含负号）。
    ///
    /// **不预读空白**：parse_term 的隐式乘法依据「`parse_factor` 返回时指针停在
    /// 因子末尾」来判断有无空白分隔；此处若先 `skip_ws` 会吞掉间隔空格，
    /// 把 `1 2` 误判成 `1*2`。
    fn parse_power(&mut self) -> Result<Expr, String> {
        let base = self.parse_primary()?;
        if self.peek() == Some('^') {
            self.pos += 1;
            self.skip_ws();
            let exp = self.parse_factor()?;
            return Ok(Expr::Pow(Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    /// primary := number | 'x' | func '(' expr ')' | '(' expr ')'
    fn parse_primary(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        let c = self.peek().ok_or_else(|| self.err("表达式意外结束"))?;
        match c {
            '0'..='9' | '.' => self.parse_number(),
            'x' | 'X' => {
                self.pos += 1;
                Ok(Expr::Var)
            }
            '(' => {
                self.pos += 1;
                let inner = self.parse_expr()?;
                self.skip_ws();
                if self.peek() != Some(')') {
                    return Err(self.err("缺少右括号 )"));
                }
                self.pos += 1;
                Ok(inner)
            }
            c if c.is_alphabetic() => {
                // 函数名：sin / cos / tan。
                let name: String = self.chars[self.pos..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                self.pos += name.len();
                let f = match name.to_lowercase().as_str() {
                    "sin" => Expr::Sin,
                    "cos" => Expr::Cos,
                    "tan" => Expr::Tan,
                    _ => return Err(self.err(&format!("未知函数 '{name}'（支持 sin/cos/tan）"))),
                };
                self.skip_ws();
                if self.peek() != Some('(') {
                    return Err(self.err(&format!("函数 '{name}' 后应有 '('")));
                }
                self.pos += 1;
                let arg = self.parse_expr()?;
                self.skip_ws();
                if self.peek() != Some(')') {
                    return Err(self.err("缺少右括号 )"));
                }
                self.pos += 1;
                Ok(f(Box::new(arg)))
            }
            other => Err(self.err(&format!("无法识别的字符 '{other}'"))),
        }
    }

    fn parse_number(&mut self) -> Result<Expr, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<f32>()
            .map(Expr::Const)
            .map_err(|_| self.err(&format!("无效数字 '{s}'")))
    }
}

/// 剥离形如 `y =`、`Y =`、`f(x) =` 的等号前缀，仅保留右侧表达式本体。
///
/// 仅当确实含 `=` 且 `=` 前片段只由字母 / 空格 / 括号组成时才剥离，
/// 避免误删独立变量名（如 `x`、`a`）。
fn strip_equation_prefix(input: &str) -> &str {
    let s = input.trim();
    match s.find('=') {
        Some(eq) => {
            let prefix = &s[..eq];
            if !prefix.is_empty()
                && prefix
                    .chars()
                    .all(|c| c.is_alphabetic() || c.is_whitespace() || c == '(' || c == ')' || c == '_')
            {
                return s[eq + 1..].trim();
            }
            s
        }
        None => s,
    }
}

/// 解析表达式字符串为 [`Expr`]。失败返回带位置的清晰错误信息。
///
/// 自动剥离 `y =` / `f(x) =` 前缀并忽略空白。
pub fn parse(input: &str) -> Result<Expr, String> {
    let body = strip_equation_prefix(input);
    let mut p = Parser::new(body);
    let e = p.parse_expr()?;
    p.skip_ws();
    if p.pos < p.chars.len() {
        return Err(p.err("表达式后有多余内容"));
    }
    Ok(e)
}

/// 在 `x_min..=x_max` 上采样 `count` 个点，返回 `(x, y)` 序列（跳过非有限值）。
pub fn sample_points(expr: &Expr, x_min: f32, x_max: f32, count: usize) -> Vec<(f32, f32)> {
    if count == 0 || x_max <= x_min {
        return Vec::new();
    }
    let step = (x_max - x_min) / (count as f32 - 1.0);
    let mut pts = Vec::with_capacity(count);
    for i in 0..count {
        let x = x_min + step * i as f32;
        let y = expr.eval(x);
        if y.is_finite() {
            pts.push((x, y));
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用户要求：`parse_linear_expr` —— "2x+1" 解析为正确 AST。
    #[test]
    fn parse_linear_expr() {
        let e = parse("2x + 1").expect("parse linear");
        // 结构上应是最外层 Add(2x, 1)：加法优先于隐式乘法。
        match &e {
            Expr::Add(a, b) => {
                assert!(matches!(**a, Expr::Mul(_, _)), "2x 应为乘法结点");
                assert!(matches!(**b, Expr::Const(1.0)), "+1 应为常量");
            }
            other => panic!("2x+1 顶层应为 Add，实际 {other:?}"),
        }
        // 求值校验。
        for (x, y) in [(0.0, 1.0), (3.0, 7.0), (-2.0, -3.0)] {
            assert!((e.eval(x) - y).abs() < 1e-5, "f({x}) = {}", e.eval(x));
        }
    }

    /// 用户要求：`eval_sin_expr` —— sin(x) 在 x=π/2 时 ≈ 1.0。
    #[test]
    fn eval_sin_expr() {
        let e = parse("sin(x)").expect("parse sin");
        let v = e.eval(std::f32::consts::FRAC_PI_2);
        assert!((v - 1.0).abs() < 1e-4, "sin(π/2) ≈ 1.0，实际 {v}");
    }

    /// 用户要求：`function_plot_generates_points` —— 采样 200 点且 y 值正确。
    #[test]
    fn function_plot_generates_points() {
        let e = parse("y = 2x").expect("parse");
        let pts = sample_points(&e, -10.0, 10.0, 200);
        assert_eq!(pts.len(), 200, "应生成 200 个采样点");
        for (x, y) in &pts {
            assert!((*y - 2.0 * x).abs() < 1e-3, "x={x} 处 y 应为 2x，实际 {y}");
        }
        // 抛物线采样同样正确。
        let e = parse("x^2").expect("parse power");
        let pts = sample_points(&e, -5.0, 5.0, 200);
        assert_eq!(pts.len(), 200);
        assert!((pts[200 - 1].0 - 5.0).abs() < 1e-5, "右端点应为 +5");
        assert!((pts[0].1 - 25.0).abs() < 1e-3, "x=-5 处 y=25，实际 {}", pts[0].1);
    }

    /// 用户要求：`parse_expr_add` —— 加法/乘法的解析与求值。
    #[test]
    fn parse_expr_add() {
        let e = parse("2 + 3 * x").expect("parse");
        assert!((e.eval(2.0) - 8.0).abs() < 1e-5, "2+3*2=8, got {}", e.eval(2.0));
        // 隐式乘法：2x。
        let e = parse("2x + 1").expect("parse implicit mul");
        assert!((e.eval(3.0) - 7.0).abs() < 1e-5);
    }

    /// 用户要求：`function_plot_points` —— 采样点数量与端点。
    #[test]
    fn function_plot_points() {
        let e = parse("x").expect("parse");
        let pts = sample_points(&e, -10.0, 10.0, 200);
        assert_eq!(pts.len(), 200, "should sample 200 finite points");
        assert!((pts[0].0 + 10.0).abs() < 1e-5);
        assert!((pts[199].0 - 10.0).abs() < 1e-5);
        // 线性函数：y = 2x，点 (x, 2x)。
        let e = parse("2x").expect("parse");
        let pts = sample_points(&e, 0.0, 1.0, 11);
        assert_eq!(pts.len(), 11);
        assert!((pts[5].1 - 1.0).abs() < 1e-5, "y=2*0.5=1, got {}", pts[5].1);
    }

    #[test]
    fn parse_sin_cos_tan() {
        let e = parse("sin(x)").expect("parse");
        assert!((e.eval(0.0) - 0.0).abs() < 1e-5);
        assert!((e.eval(std::f32::consts::FRAC_PI_2) - 1.0).abs() < 1e-4);
        let e = parse("cos(0)").expect("parse const arg");
        assert!((e.eval(0.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn parse_nested_and_parens() {
        let e = parse("(x + 1) * (x - 1)").expect("parse");
        assert!((e.eval(2.0) - 3.0).abs() < 1e-5, "(2+1)*(2-1)=3");
        // 函数嵌套：sin(x)*cos(x)。
        let e = parse("sin(x) * cos(x)").expect("parse");
        assert!((e.eval(0.0) - 0.0).abs() < 1e-5);
    }

    #[test]
    fn parse_errors_are_clear() {
        assert!(parse("2 +").is_err(), "trailing operator should fail");
        assert!(parse("(x").is_err(), "missing closing paren should fail");
        assert!(parse("foo(x)").is_err(), "unknown function should fail");
        assert!(parse("1 2").is_err(), "trailing garbage should fail");
    }

    #[test]
    fn parse_division_by_zero_is_nan() {
        let e = parse("1 / x").expect("parse");
        assert!(e.eval(0.0).is_nan(), "1/0 should be NaN, sampled out");
        assert!((e.eval(2.0) - 0.5).abs() < 1e-5);
    }

    /// 幂运算 `^`：`y = x^2` 应解析为抛物线并正确求值。
    #[test]
    fn parse_power_parabola() {
        let e = parse("y = x^2").expect("parse prefix + power");
        assert_eq!(e.eval(0.0), 0.0);
        assert!((e.eval(2.0) - 4.0).abs() < 1e-5);
        assert!((e.eval(-3.0) - 9.0).abs() < 1e-5);
        // 采样得到抛物线。
        let pts = sample_points(&e, -2.0, 2.0, 5);
        assert_eq!(pts.len(), 5);
        for (x, y) in pts {
            assert!((y - x * x).abs() < 1e-3, "x={x} 处 y 应为 x^2");
        }
    }

    /// 幂运算右结合：`2^3^2` = `2^(3^2)` = 2^9 = 512（而非 64）。
    #[test]
    fn parse_power_right_assoc() {
        let e = parse("2^3^2").expect("parse right-assoc power");
        assert!((e.eval(0.0) - 512.0).abs() < 1e-3);
    }

    /// 前缀剥离：`y = 2x + 1` 与 `Y=2x+1` 求值一致。
    #[test]
    fn parse_strips_equation_prefix() {
        let a = parse("y = 2x + 1").expect("lowercase prefix");
        let b = parse("  Y= 2x+1  ").expect("uppercase prefix, no spaces");
        for (x, expected) in [(0.0, 1.0), (3.0, 7.0), (-2.0, -3.0)] {
            assert!((a.eval(x) - expected).abs() < 1e-5, "a({x}) = {}", a.eval(x));
            assert!((b.eval(x) - expected).abs() < 1e-5, "b({x}) = {}", b.eval(x));
        }
        // f(x) = ... 形式。
        let c = parse("f(x) = x^2").expect("f(x) prefix");
        assert!((c.eval(4.0) - 16.0).abs() < 1e-5);
    }

    /// 一元负号比幂结合得更松：`-2^2` = `-(2^2)` = -4。
    #[test]
    fn parse_unary_minus_binds_looser_than_power() {
        let e = parse("-2^2").expect("parse");
        assert!((e.eval(0.0) + 4.0).abs() < 1e-5);
    }
}
