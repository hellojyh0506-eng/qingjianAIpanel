//! 小型 SVG path 解析器：支持 `M L H V C S Q T A Z` 及其相对坐标小写形式，圆弧转成三次贝塞尔。
//!
//! 只服务内嵌的 Lucide 图标：输入是可信的常量，解析失败（语法不对）返回 `None`，由测试保证所有图标都能解析。

use tiny_skia::PathBuilder;

/// 把 `d` 里的所有子路径追加到 `builder`；语法有误返回 `None`（已追加的部分不回滚）。
pub(super) fn append(builder: &mut PathBuilder, d: &str) -> Option<()> {
    let mut scan = Scanner::new(d);
    let mut state = State::default();
    let mut command = None;
    loop {
        scan.skip_separators();
        let Some(next) = scan.peek() else {
            return Some(());
        };
        if next.is_ascii_alphabetic() {
            command = Some(next);
            scan.advance();
            if matches!(next, 'Z' | 'z') {
                builder.close();
                (state.x, state.y) = (state.start_x, state.start_y);
                state.last_control = None;
                continue;
            }
        } else {
            // 命令字母之后可以连续跟多组参数；M 之后的隐式命令是 L
            command = Some(match command? {
                'M' => 'L',
                'm' => 'l',
                other => other,
            });
        }
        run(builder, &mut scan, &mut state, command?)?;
    }
}

#[derive(Default)]
struct State {
    x: f32,
    y: f32,
    start_x: f32,
    start_y: f32,

    /// 上一条曲线的末控制点，S / T 用来取镜像；上一条不是对应曲线则为 `None`。
    last_control: Option<(f32, f32, ControlKind)>,
}

#[derive(Clone, Copy, PartialEq)]
enum ControlKind {
    Cubic,
    Quad,
}

/// 读一组参数并执行一条命令。
fn run(b: &mut PathBuilder, scan: &mut Scanner, st: &mut State, cmd: char) -> Option<()> {
    let rel = cmd.is_ascii_lowercase();
    let (ox, oy) = if rel { (st.x, st.y) } else { (0.0, 0.0) };
    match cmd.to_ascii_uppercase() {
        'M' => {
            let (x, y) = (scan.number()? + ox, scan.number()? + oy);
            b.move_to(x, y);
            (st.x, st.y, st.start_x, st.start_y) = (x, y, x, y);
            st.last_control = None;
        }
        'L' => {
            let (x, y) = (scan.number()? + ox, scan.number()? + oy);
            b.line_to(x, y);
            (st.x, st.y) = (x, y);
            st.last_control = None;
        }
        'H' => {
            let x = scan.number()? + ox;
            b.line_to(x, st.y);
            st.x = x;
            st.last_control = None;
        }
        'V' => {
            let y = scan.number()? + oy;
            b.line_to(st.x, y);
            st.y = y;
            st.last_control = None;
        }
        'C' => {
            let (x1, y1) = (scan.number()? + ox, scan.number()? + oy);
            let (x2, y2) = (scan.number()? + ox, scan.number()? + oy);
            let (x, y) = (scan.number()? + ox, scan.number()? + oy);
            b.cubic_to(x1, y1, x2, y2, x, y);
            (st.x, st.y) = (x, y);
            st.last_control = Some((x2, y2, ControlKind::Cubic));
        }
        'S' => {
            let (x1, y1) = reflected(st, ControlKind::Cubic);
            let (x2, y2) = (scan.number()? + ox, scan.number()? + oy);
            let (x, y) = (scan.number()? + ox, scan.number()? + oy);
            b.cubic_to(x1, y1, x2, y2, x, y);
            (st.x, st.y) = (x, y);
            st.last_control = Some((x2, y2, ControlKind::Cubic));
        }
        'Q' => {
            let (x1, y1) = (scan.number()? + ox, scan.number()? + oy);
            let (x, y) = (scan.number()? + ox, scan.number()? + oy);
            b.quad_to(x1, y1, x, y);
            (st.x, st.y) = (x, y);
            st.last_control = Some((x1, y1, ControlKind::Quad));
        }
        'T' => {
            let (x1, y1) = reflected(st, ControlKind::Quad);
            let (x, y) = (scan.number()? + ox, scan.number()? + oy);
            b.quad_to(x1, y1, x, y);
            (st.x, st.y) = (x, y);
            st.last_control = Some((x1, y1, ControlKind::Quad));
        }
        'A' => {
            let rx = scan.number()?;
            let ry = scan.number()?;
            let rotation = scan.number()?;
            let large = scan.flag()?;
            let sweep = scan.flag()?;
            let (x, y) = (scan.number()? + ox, scan.number()? + oy);
            arc_to(b, (st.x, st.y), (rx, ry), rotation, large, sweep, (x, y));
            (st.x, st.y) = (x, y);
            st.last_control = None;
        }
        _ => return None,
    }
    Some(())
}

/// S / T 的第一个控制点：上一条同类曲线末控制点关于当前点的镜像，否则就是当前点。
fn reflected(st: &State, kind: ControlKind) -> (f32, f32) {
    match st.last_control {
        Some((cx, cy, k)) if k == kind => (2.0 * st.x - cx, 2.0 * st.y - cy),
        _ => (st.x, st.y),
    }
}

/// 端点式圆弧 → 若干段三次贝塞尔（SVG 规范 F.6.5 的端点转圆心参数，再按不超过 90° 切段）。
fn arc_to(
    b: &mut PathBuilder,
    from: (f32, f32),
    radii: (f32, f32),
    rotation_deg: f32,
    large: bool,
    sweep: bool,
    to: (f32, f32),
) {
    let (mut rx, mut ry) = (radii.0.abs() as f64, radii.1.abs() as f64);
    let (x1, y1) = (from.0 as f64, from.1 as f64);
    let (x2, y2) = (to.0 as f64, to.1 as f64);
    if (x1 == x2 && y1 == y2) || rx == 0.0 || ry == 0.0 {
        if (x1, y1) != (x2, y2) {
            b.line_to(to.0, to.1);
        }
        return;
    }
    let phi = (rotation_deg as f64).to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    // 半径太小够不到终点时按比例放大
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coefficient = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        coefficient = -coefficient;
    }
    let cxp = coefficient * rx * y1p / ry;
    let cyp = -coefficient * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let cross = ux * vy - uy * vx;
        cross.atan2(dot)
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let segments = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / segments as f64;
    let t = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |theta: f64| {
        let (s, c) = theta.sin_cos();
        (
            cx + rx * c * cos_phi - ry * s * sin_phi,
            cy + rx * c * sin_phi + ry * s * cos_phi,
        )
    };
    let derivative = |theta: f64| {
        let (s, c) = theta.sin_cos();
        (
            -rx * s * cos_phi - ry * c * sin_phi,
            -rx * s * sin_phi + ry * c * cos_phi,
        )
    };
    let mut theta = theta1;
    for i in 0..segments {
        let next = theta + step;
        let (p0x, p0y) = point(theta);
        let (d0x, d0y) = derivative(theta);
        let (p3x, p3y) = point(next);
        let (d3x, d3y) = derivative(next);
        // 最后一段落在精确终点，避免累计误差
        let end = if i + 1 == segments {
            (to.0, to.1)
        } else {
            (p3x as f32, p3y as f32)
        };
        b.cubic_to(
            (p0x + t * d0x) as f32,
            (p0y + t * d0y) as f32,
            (p3x - t * d3x) as f32,
            (p3y - t * d3y) as f32,
            end.0,
            end.1,
        );
        theta = next;
    }
}

/// 逐字符读数字与圆弧标志。
struct Scanner<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Scanner<'a> {
    fn new(d: &'a str) -> Self {
        Self {
            chars: d.chars().peekable(),
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn advance(&mut self) {
        self.chars.next();
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_ascii_whitespace() || c == ',') {
            self.advance();
        }
    }

    /// 圆弧标志：单个字符 `0` / `1`，后面可以紧贴下一个参数。
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        match self.peek()? {
            '0' => {
                self.advance();
                Some(false)
            }
            '1' => {
                self.advance();
                Some(true)
            }
            _ => None,
        }
    }

    /// 数字：可带符号、小数点、指数；`1.5.5` 读作 `1.5` 与 `.5`，`1-2` 读作 `1` 与 `-2`。
    fn number(&mut self) -> Option<f32> {
        self.skip_separators();
        let mut text = String::new();
        if matches!(self.peek(), Some('+' | '-')) {
            text.extend(self.chars.next());
        }
        let mut seen_dot = false;
        let mut seen_digit = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                seen_digit = true;
            } else if c == '.' && !seen_dot {
                seen_dot = true;
            } else {
                break;
            }
            text.push(c);
            self.advance();
        }
        if !seen_digit {
            return None;
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            text.extend(self.chars.next());
            if matches!(self.peek(), Some('+' | '-')) {
                text.extend(self.chars.next());
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                text.extend(self.chars.next());
            }
        }
        text.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(d: &str) -> Option<tiny_skia::Path> {
        let mut builder = PathBuilder::new();
        append(&mut builder, d)?;
        builder.finish()
    }

    fn bounds(d: &str) -> (f32, f32, f32, f32) {
        let b = build(d).unwrap().bounds();
        (b.left(), b.top(), b.right(), b.bottom())
    }

    #[test]
    fn absolute_and_relative_lines_agree() {
        assert_eq!(bounds("M2 3L12 3L12 8L2 8Z"), (2.0, 3.0, 12.0, 8.0));
        assert_eq!(bounds("m2 3h10v5H2z"), (2.0, 3.0, 12.0, 8.0));
    }

    #[test]
    fn implicit_lineto_after_moveto_and_compact_numbers() {
        // M 后面多组坐标等价于 L；`1-2`、`.5.5` 这种紧凑写法
        assert_eq!(bounds("M0 0 10 0 10 5"), (0.0, 0.0, 10.0, 5.0));
        assert_eq!(bounds("M1-2l.5.5"), (1.0, -2.0, 1.5, -1.5));
    }

    #[test]
    fn close_returns_to_subpath_start_for_following_relative_moves() {
        // z 之后的相对命令以子路径起点为基准
        assert_eq!(bounds("M4 4h4v4z l3 0"), (4.0, 4.0, 8.0, 8.0));
    }

    #[test]
    fn full_circle_from_two_arcs_has_expected_bounds() {
        let (l, t, r, b) = bounds("M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0z");
        for (got, want) in [(l, 2.0), (t, 2.0), (r, 22.0), (b, 22.0)] {
            assert!((got - want).abs() < 0.05, "{got} vs {want}");
        }
    }

    #[test]
    fn arc_flags_may_touch_the_next_number() {
        // Lucide 压缩过的路径会写成 `a2 2 0 011 1`
        let (l, t, r, b) = bounds("M0 0a1 1 0 011 1");
        assert!(l >= -0.001 && t >= -0.001 && r <= 1.001 && b <= 1.001);
        assert!(r > 0.9 && b > 0.9);
    }

    #[test]
    fn arc_with_too_small_radius_is_scaled_up_to_reach_the_end() {
        let (l, _, r, _) = bounds("M0 0A1 1 0 0 1 10 0");
        assert!(l >= -0.001 && (r - 10.0).abs() < 0.001);
    }

    #[test]
    fn smooth_curves_mirror_the_previous_control_point() {
        assert!(build("M0 0C0 5 5 5 5 0S10-5 10 0").is_some());
        assert!(build("M0 0Q2 4 4 0T8 0").is_some());
    }

    #[test]
    fn rejects_garbage() {
        assert!(build("M0").is_none());
        assert!(build("10 20").is_none());
        assert!(build("M0 0 X 1").is_none());
    }
}
