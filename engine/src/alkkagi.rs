//! 알까기 물리 시뮬레이션.
//!
//! 판은 1.0 x 1.0, 돌 중심이 판 밖으로 나가면 탈락. 마찰은 일정한 감속도로 모델링하므로
//! 힘 p로 튕긴 돌은 장애물이 없을 때 정확히 `TRAVEL_MAX * p^2` 만큼 미끄러진다.

use serde::{Deserialize, Serialize};

pub const RADIUS: f64 = 0.03;
pub const TRAVEL_MAX: f64 = 1.6;
const DECEL: f64 = 0.9;
const RESTITUTION: f64 = 0.92;
const DT: f64 = 1.0 / 240.0;
const FRAME_EVERY: usize = 8; // 30fps로 기록
const MAX_TIME: f64 = 15.0;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Stone {
    pub id: u32,
    /// 1 흑, 2 백
    pub color: u8,
    pub x: f64,
    pub y: f64,
    pub alive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AlkState {
    pub stones: Vec<Stone>,
    pub to_play: u8,
    /// 1 흑 승, 2 백 승
    pub winner: Option<u8>,
    pub turn: u32,
}

#[derive(Serialize, Debug)]
pub struct ShotOption {
    pub id: usize,
    pub shooter: u32,
    pub target: u32,
    /// 라디안, 화면 좌표계(y가 아래로 증가)
    pub angle: f64,
    pub distance: f64,
    pub target_exit: f64,
    pub blocked: bool,
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn on_board(x: f64, y: f64) -> bool {
    (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)
}

/// (x,y)에서 방향 (dx,dy)로 판 끝까지 거리
fn exit_distance(x: f64, y: f64, dx: f64, dy: f64) -> f64 {
    let mut t = f64::INFINITY;
    if dx > 1e-9 { t = t.min((1.0 - x) / dx); }
    if dx < -1e-9 { t = t.min(-x / dx); }
    if dy > 1e-9 { t = t.min((1.0 - y) / dy); }
    if dy < -1e-9 { t = t.min(-y / dy); }
    t
}

impl AlkState {
    pub fn new(count: usize) -> Result<Self, String> {
        if !(1..=8).contains(&count) {
            return Err("돌 개수는 1~8개여야 합니다".into());
        }
        let mut stones = vec![];
        for (color, y) in [(1u8, 0.8), (2u8, 0.2)] {
            for k in 0..count {
                let x = if count == 1 { 0.5 } else { 0.15 + 0.7 * k as f64 / (count - 1) as f64 };
                stones.push(Stone { id: stones.len() as u32, color, x, y, alive: true });
            }
        }
        Ok(AlkState { stones, to_play: 1, winner: None, turn: 0 })
    }

    fn alive_count(&self, color: u8) -> usize {
        self.stones.iter().filter(|s| s.alive && s.color == color).count()
    }

    /// color 쪽이 쏠 수 있는 (내 돌 → 상대 돌) 직선 샷 후보
    pub fn options(&self, color: u8) -> Vec<ShotOption> {
        let mut out = vec![];
        for s in self.stones.iter().filter(|s| s.alive && s.color == color) {
            for t in self.stones.iter().filter(|t| t.alive && t.color != color) {
                let (dx, dy) = (t.x - s.x, t.y - s.y);
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < 1e-6 {
                    continue;
                }
                let (ux, uy) = (dx / dist, dy / dist);
                let blocked = self.stones.iter().any(|u| {
                    if !u.alive || u.id == s.id || u.id == t.id {
                        return false;
                    }
                    let (wx, wy) = (u.x - s.x, u.y - s.y);
                    let proj = wx * ux + wy * uy;
                    let perp = (wx * uy - wy * ux).abs();
                    proj > 0.0 && proj < dist && perp < 2.0 * RADIUS
                });
                out.push(ShotOption {
                    id: 0,
                    shooter: s.id,
                    target: t.id,
                    angle: round4(dy.atan2(dx)),
                    distance: round4(dist),
                    target_exit: round4(exit_distance(t.x, t.y, ux, uy)),
                    blocked,
                });
            }
        }
        out.sort_by(|a, b| {
            (a.blocked, a.distance + a.target_exit)
                .partial_cmp(&(b.blocked, b.distance + b.target_exit))
                .unwrap()
        });
        out.truncate(24);
        for (i, o) in out.iter_mut().enumerate() {
            o.id = i;
        }
        out
    }

    /// 돌 하나를 쏘고 멈출 때까지 시뮬레이션. 프레임마다 [x, y, alive(1/0)] 배열을 반환.
    pub fn shoot(&mut self, stone_id: u32, angle: f64, power: f64) -> Result<Vec<Vec<[f64; 3]>>, String> {
        if self.winner.is_some() {
            return Err("이미 끝난 게임입니다".into());
        }
        let si = self
            .stones
            .iter()
            .position(|s| s.id == stone_id && s.alive)
            .ok_or("없는 돌입니다")?;
        if self.stones[si].color != self.to_play {
            return Err("자기 돌만 쏠 수 있습니다".into());
        }
        if !angle.is_finite() || !power.is_finite() {
            return Err("잘못된 각도/힘".into());
        }
        let p = power.clamp(0.05, 1.0);
        let v0 = (2.0 * DECEL * TRAVEL_MAX).sqrt() * p;
        let n = self.stones.len();
        let mut vel = vec![(0.0f64, 0.0f64); n];
        vel[si] = (v0 * angle.cos(), v0 * angle.sin());

        let snapshot = |st: &[Stone]| st.iter().map(|s| [round4(s.x), round4(s.y), s.alive as u8 as f64]).collect::<Vec<_>>();
        let mut frames = vec![snapshot(&self.stones)];
        let mut step = 0usize;
        let mut t = 0.0;
        loop {
            // 이동 + 마찰
            for (i, s) in self.stones.iter_mut().enumerate() {
                if !s.alive {
                    continue;
                }
                let (vx, vy) = vel[i];
                let sp = (vx * vx + vy * vy).sqrt();
                if sp == 0.0 {
                    continue;
                }
                s.x += vx * DT;
                s.y += vy * DT;
                let ns = (sp - DECEL * DT).max(0.0);
                vel[i] = (vx * ns / sp, vy * ns / sp);
            }
            // 충돌 (같은 질량, 반발 계수 RESTITUTION)
            for i in 0..n {
                for j in i + 1..n {
                    let (a, b) = (&self.stones[i], &self.stones[j]);
                    if !a.alive || !b.alive {
                        continue;
                    }
                    let (dx, dy) = (b.x - a.x, b.y - a.y);
                    let d = (dx * dx + dy * dy).sqrt();
                    if d >= 2.0 * RADIUS || d < 1e-9 {
                        continue;
                    }
                    let (nx, ny) = (dx / d, dy / d);
                    let rel = (vel[i].0 - vel[j].0) * nx + (vel[i].1 - vel[j].1) * ny;
                    if rel > 0.0 {
                        let imp = rel * (1.0 + RESTITUTION) / 2.0;
                        vel[i].0 -= imp * nx;
                        vel[i].1 -= imp * ny;
                        vel[j].0 += imp * nx;
                        vel[j].1 += imp * ny;
                    }
                    // 겹침 해소
                    let push = (2.0 * RADIUS - d) / 2.0;
                    self.stones[i].x -= nx * push;
                    self.stones[i].y -= ny * push;
                    self.stones[j].x += nx * push;
                    self.stones[j].y += ny * push;
                }
            }
            // 판 밖으로 나간 돌 탈락
            for (i, s) in self.stones.iter_mut().enumerate() {
                if s.alive && !on_board(s.x, s.y) {
                    s.alive = false;
                    vel[i] = (0.0, 0.0);
                }
            }
            step += 1;
            t += DT;
            let moving = self.stones.iter().zip(&vel).any(|(s, v)| s.alive && (v.0 != 0.0 || v.1 != 0.0));
            if step % FRAME_EVERY == 0 || !moving {
                frames.push(snapshot(&self.stones));
            }
            if !moving || t > MAX_TIME {
                break;
            }
        }

        let shooter = self.to_play;
        let (mine, theirs) = (self.alive_count(shooter), self.alive_count(3 - shooter));
        if theirs == 0 && mine > 0 {
            self.winner = Some(shooter);
        } else if mine == 0 {
            // 내 돌이 다 떨어지면 (동시에 전멸해도) 상대 승
            self.winner = Some(3 - shooter);
        }
        self.to_play = 3 - shooter;
        self.turn += 1;
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_travel_matches_formula() {
        let mut st = AlkState::new(1).unwrap();
        st.stones[1].alive = false; // 백 돌 제거
        st.stones[0].x = 0.1;
        st.stones[0].y = 0.5;
        st.shoot(0, 0.0, 0.5).unwrap(); // 오른쪽으로 0.4 이동 예상
        let moved = st.stones[0].x - 0.1;
        assert!((moved - 0.4).abs() < 0.01, "moved {moved}");
    }

    #[test]
    fn head_on_hit_knocks_out() {
        let mut st = AlkState::new(1).unwrap(); // 흑 (0.5,0.8), 백 (0.5,0.2)
        let opts = st.options(1);
        assert_eq!(opts.len(), 1);
        let o = &opts[0];
        assert!((o.distance - 0.6).abs() < 1e-6);
        assert!((o.target_exit - 0.2).abs() < 1e-6);
        st.shoot(o.shooter, o.angle, 1.0).unwrap();
        assert!(!st.stones[1].alive, "백 돌이 떨어져야 함");
        assert!(st.stones[0].alive, "흑 돌은 남아야 함");
        assert_eq!(st.winner, Some(1));
    }

    #[test]
    fn cannot_shoot_opponent_stone() {
        let mut st = AlkState::new(3).unwrap();
        assert!(st.shoot(3, 0.0, 0.5).is_err());
    }
}
