//! 终端二维码渲染。
//!
//! 用**半块字符**：一个字符格画上下两个模块。终端字符通常是 1:2 的长宽比，一格一列、
//! 两行一格正好把二维码还原成近似正方形——否则码会被拉长一倍，扫码器识别率骤降。
//!
//! 颜色是**反的**：深模块留空、浅模块用实心块。终端多为深色背景，这样「亮 = 浅模块」，
//! 与纸面二维码的明暗关系一致；直接照搬纸面配色会得到一张扫不出来的负片。

/// 终端二维码的目标模块数上限。
///
/// 半块渲染下 1 模块 = 1 字符宽，所以这个数就是二维码占的列数；取 80 是为了在标准宽度的
/// 终端里完整显示——超出终端宽度的码会被换行截断，扫不出来。
/// `invite_qr_*` 按「每模块最少 2 px」换算容量，故传 2 倍。
pub const FACE_PX: u32 = 80 * swarmdrop_invite::MIN_PX_PER_MODULE;

/// 把模块矩阵画成可打印的字符串（不含尾随换行）。
pub fn render(matrix: &[Vec<bool>]) -> String {
    let mut out = String::new();
    let height = matrix.len();

    for pair in (0..height).step_by(2) {
        for col in 0..matrix[pair].len() {
            let top_dark = matrix[pair][col];
            // 奇数行高时，最后一格的下半没有对应模块——按浅模块处理，
            // 它落在 quiet zone 里，不影响识别。
            let bottom_dark = matrix.get(pair + 1).map(|r| r[col]).unwrap_or(false);
            out.push(match (top_dark, bottom_dark) {
                (false, false) => '█',
                (false, true) => '▀',
                (true, false) => '▄',
                (true, true) => ' ',
            });
        }
        out.push('\n');
    }
    out.pop();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 两行模块合成一行字符——高度减半是这套渲染成立的前提。
    #[test]
    fn two_module_rows_become_one_character_row() {
        let matrix = vec![vec![false, true], vec![true, false], vec![false, false]];
        let text = render(&matrix);
        assert_eq!(text.lines().count(), 2, "3 行模块应画成 2 行字符");
    }

    /// 明暗关系必须是反的：深模块留空、浅模块出块。
    #[test]
    fn dark_modules_render_as_blank() {
        let all_dark = render(&[vec![true], vec![true]]);
        let all_light = render(&[vec![false], vec![false]]);
        assert_eq!(all_dark, " ");
        assert_eq!(all_light, "█");
    }
}
