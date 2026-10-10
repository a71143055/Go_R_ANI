//! 바둑 AI: 몬테카를로 트리 탐색(MCTS) + RAVE.
//!
//! - 빠른 플레이아웃 전용 판(`Board`): 체인 연결 리스트 + 의사 활로(pseudo-liberty)
//! - 플레이아웃 정책: 직전 수 주변 따내기/단수 살리기 → 무작위(집눈 메우기·자충 회피)
//! - 트리: 휴리스틱 사전 지식(prior) + RAVE, 여러 스레드가 독립 트리를 키운 뒤 결과를 합친다
//! - 종국 사석 판정: 현재 판에서 플레이아웃을 반복해 각 돌의 소유 확률을 추정한다

use crate::coord;
use crate::go::{GoState, BLACK, EMPTY, WHITE};
use serde::Serialize;
use std::time::{Duration, Instant};

const BORDER: u8 = 3;
pub const PASS: usize = 0;
const EXPAND_AT: f32 = 6.0;
const RAVE_EQ: f32 = 2500.0;
const MAX_NODES_PER_THREAD: usize = 700_000;

// ------------------------------------------------------------------ 난수

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
}

// ------------------------------------------------------------------ 빠른 판

#[derive(Clone)]
pub struct Board {
    pub n: usize,
    w: usize,
    color: Vec<u8>,
    head: Vec<usize>,
    next: Vec<usize>,
    plib: Vec<i32>,
    csize: Vec<u32>,
    empties: Vec<usize>,
    eidx: Vec<usize>,
    pub ko: usize,
    pub to_play: u8,
    pub passes: u8,
    pub last: usize,
    pub komi: f32,
}

impl Board {
    pub fn from_state(st: &GoState) -> Self {
        let n = st.size;
        let w = n + 2;
        let len = w * w;
        let mut b = Board {
            n,
            w,
            color: vec![BORDER; len],
            head: vec![0; len],
            next: vec![0; len],
            plib: vec![0; len],
            csize: vec![0; len],
            empties: Vec::with_capacity(n * n),
            eidx: vec![0; len],
            ko: PASS,
            to_play: st.to_play,
            passes: st.passes.min(2) as u8,
            last: PASS,
            komi: st.komi,
        };
        for y in 0..n {
            for x in 0..n {
                let p = b.pt(x, y);
                b.color[p] = st.board[y * n + x];
                if b.color[p] == EMPTY {
                    b.add_empty(p);
                }
            }
        }
        let mut seen = vec![false; len];
        for start in 0..len {
            let c = b.color[start];
            if (c != BLACK && c != WHITE) || seen[start] {
                continue;
            }
            let mut stones = vec![start];
            seen[start] = true;
            let mut i = 0;
            while i < stones.len() {
                let s = stones[i];
                i += 1;
                for q in b.nbs(s) {
                    if b.color[q] == c && !seen[q] {
                        seen[q] = true;
                        stones.push(q);
                    }
                }
            }
            let mut libs = 0;
            for (k, &s) in stones.iter().enumerate() {
                b.head[s] = start;
                b.next[s] = stones[(k + 1) % stones.len()];
                libs += b.nbs(s).iter().filter(|&&q| b.color[q] == EMPTY).count() as i32;
            }
            b.plib[start] = libs;
            b.csize[start] = stones.len() as u32;
        }
        if let Some(k) = st.ko {
            b.ko = b.pt(k % n, k / n);
        }
        if let Some(l) = st.last {
            b.last = b.pt(l % n, l / n);
        }
        b
    }

    #[inline]
    pub fn pt(&self, x: usize, y: usize) -> usize {
        (y + 1) * self.w + x + 1
    }
    #[inline]
    pub fn xy(&self, p: usize) -> (usize, usize) {
        (p % self.w - 1, p / self.w - 1)
    }
    /// 내부 좌표 → go.rs 인덱스(y*n+x)
    pub fn to_idx(&self, p: usize) -> usize {
        let (x, y) = self.xy(p);
        y * self.n + x
    }
    #[inline]
    fn nbs(&self, p: usize) -> [usize; 4] {
        [p - 1, p + 1, p - self.w, p + self.w]
    }
    #[inline]
    fn diags(&self, p: usize) -> [usize; 4] {
        [p - self.w - 1, p - self.w + 1, p + self.w - 1, p + self.w + 1]
    }
    fn add_empty(&mut self, p: usize) {
        self.eidx[p] = self.empties.len();
        self.empties.push(p);
    }
    fn remove_empty(&mut self, p: usize) {
        let i = self.eidx[p];
        let last = *self.empties.last().unwrap();
        self.empties[i] = last;
        self.eidx[last] = i;
        self.empties.pop();
    }
    pub fn empty_count(&self) -> usize {
        self.empties.len()
    }

    /// 체인에 p 말고 다른 활로가 있는가
    fn has_lib_except(&self, h: usize, p: usize) -> bool {
        // p에 맞닿은 칸은 최대 4개이므로 의사 활로가 4를 넘으면 다른 활로가 반드시 있다
        if self.plib[h] > 4 {
            return true;
        }
        let mut s = h;
        loop {
            for q in self.nbs(s) {
                if q != p && self.color[q] == EMPTY {
                    return true;
                }
            }
            s = self.next[s];
            if s == h {
                return false;
            }
        }
    }

    /// 체인의 활로를 최대 2개까지 센다. (개수, 첫 활로)
    fn libs2(&self, h: usize) -> (u8, usize) {
        let mut first = 0;
        let mut k = 0u8;
        let mut s = h;
        loop {
            for q in self.nbs(s) {
                if self.color[q] == EMPTY && (k == 0 || q != first) {
                    if k == 0 {
                        first = q;
                        k = 1;
                    } else {
                        return (2, first);
                    }
                }
            }
            s = self.next[s];
            if s == h {
                return (k, first);
            }
        }
    }

    pub fn is_legal(&self, p: usize, c: u8) -> bool {
        if p == PASS {
            return true;
        }
        if self.color[p] != EMPTY || p == self.ko {
            return false;
        }
        for q in self.nbs(p) {
            let cq = self.color[q];
            if cq == EMPTY
                || (cq == c && self.has_lib_except(self.head[q], p))
                || (cq == 3 - c && !self.has_lib_except(self.head[q], p))
            {
                return true;
            }
        }
        false
    }

    /// 사방이 내 돌인 진짜 집눈 (대각선 조건 포함)
    fn is_eye(&self, p: usize, c: u8) -> bool {
        for q in self.nbs(p) {
            let cq = self.color[q];
            if cq != c && cq != BORDER {
                return false;
            }
        }
        let mut bad = 0;
        let mut edge = false;
        for q in self.diags(p) {
            match self.color[q] {
                BORDER => edge = true,
                cq if cq == 3 - c => bad += 1,
                _ => {}
            }
        }
        if edge {
            bad == 0
        } else {
            bad < 2
        }
    }

    /// p에 두었을 때 내 체인의 활로 수 (최대 2, 따내기가 생기면 2)
    fn libs_after(&self, p: usize, c: u8) -> u8 {
        let mut first = 0;
        let mut k = 0u8;
        let mut push = |q: usize, k: &mut u8| -> bool {
            if *k == 0 {
                first = q;
                *k = 1;
                false
            } else {
                q != first
            }
        };
        for q in self.nbs(p) {
            if self.color[q] == EMPTY && push(q, &mut k) {
                return 2;
            }
        }
        for q in self.nbs(p) {
            let cq = self.color[q];
            if cq == 3 - c && !self.has_lib_except(self.head[q], p) {
                return 2;
            }
            if cq == c {
                let h = self.head[q];
                let mut s = h;
                loop {
                    for r in self.nbs(s) {
                        if r != p && self.color[r] == EMPTY && push(r, &mut k) {
                            return 2;
                        }
                    }
                    s = self.next[s];
                    if s == h {
                        break;
                    }
                }
            }
        }
        k
    }

    /// 착수 (합법성은 호출자가 확인). 따낸 돌 수를 돌려준다.
    pub fn play(&mut self, p: usize) -> u32 {
        let c = self.to_play;
        self.to_play = 3 - c;
        if p == PASS {
            self.passes += 1;
            self.ko = PASS;
            self.last = PASS;
            return 0;
        }
        self.passes = 0;
        self.remove_empty(p);
        self.color[p] = c;
        self.head[p] = p;
        self.next[p] = p;
        self.csize[p] = 1;
        self.plib[p] = 0;
        for q in self.nbs(p) {
            match self.color[q] {
                EMPTY => self.plib[p] += 1,
                BLACK | WHITE => {
                    let h = self.head[q];
                    self.plib[h] -= 1;
                }
                _ => {}
            }
        }
        let mut captured = 0;
        let mut cap_pt = PASS;
        for q in self.nbs(p) {
            if self.color[q] == 3 - c && self.plib[self.head[q]] == 0 {
                captured += self.remove_chain(self.head[q]);
                cap_pt = q;
            }
        }
        for q in self.nbs(p) {
            if self.color[q] == c {
                let (a, b) = (self.head[p], self.head[q]);
                if a != b {
                    self.merge(a, b);
                }
            }
        }
        let h = self.head[p];
        self.ko = if captured == 1 && self.csize[h] == 1 && self.plib[h] == 1 { cap_pt } else { PASS };
        self.last = p;
        captured
    }

    fn remove_chain(&mut self, h: usize) -> u32 {
        let mut stones = Vec::with_capacity(self.csize[h] as usize);
        let mut s = h;
        loop {
            stones.push(s);
            s = self.next[s];
            if s == h {
                break;
            }
        }
        for &s in &stones {
            self.color[s] = EMPTY;
            self.add_empty(s);
        }
        for &s in &stones {
            for q in self.nbs(s) {
                if self.color[q] == BLACK || self.color[q] == WHITE {
                    let hq = self.head[q];
                    self.plib[hq] += 1;
                }
            }
        }
        stones.len() as u32
    }

    fn merge(&mut self, mut a: usize, mut b: usize) {
        if self.csize[a] < self.csize[b] {
            std::mem::swap(&mut a, &mut b);
        }
        let mut s = b;
        loop {
            self.head[s] = a;
            s = self.next[s];
            if s == b {
                break;
            }
        }
        let (na, nb) = (self.next[a], self.next[b]);
        self.next[a] = nb;
        self.next[b] = na;
        self.plib[a] += self.plib[b];
        self.csize[a] += self.csize[b];
    }

    /// 플레이아웃 정책으로 다음 수를 고른다
    fn policy_move(&self, rng: &mut Rng) -> usize {
        let c = self.to_play;
        if self.last != PASS {
            let l = self.last;
            // 상대가 방금 둔 돌이 단수면 따낸다
            let (k, lib) = self.libs2(self.head[l]);
            if k == 1 && self.is_legal(lib, c) {
                return lib;
            }
            for q in self.nbs(l) {
                let cq = self.color[q];
                if cq == BLACK || cq == WHITE {
                    let (k, lib) = self.libs2(self.head[q]);
                    if k != 1 || !self.is_legal(lib, c) {
                        continue;
                    }
                    // 이웃 상대 체인이 단수면 따내고, 내 체인이 단수면 달아난다
                    if cq == 3 - c || self.libs_after(lib, c) >= 2 {
                        return lib;
                    }
                }
            }
        }
        let len = self.empties.len();
        if len == 0 {
            return PASS;
        }
        let start = rng.below(len);
        for i in 0..len {
            let p = self.empties[(start + i) % len];
            if p == self.ko || self.is_eye(p, c) || !self.is_legal(p, c) {
                continue;
            }
            if self.libs_after(p, c) < 2 && rng.chance(0.85) {
                continue; // 대부분의 자충(스스로 단수) 회피
            }
            return p;
        }
        PASS
    }

    /// 끝까지 두어 본다. moves에는 (좌표, 색)을 기록.
    fn playout(&mut self, rng: &mut Rng, moves: &mut Vec<(usize, u8)>) {
        let limit = self.n * self.n * 3;
        let mut k = 0;
        while self.passes < 2 && k < limit {
            let p = self.policy_move(rng);
            moves.push((p, self.to_play));
            self.play(p);
            k += 1;
        }
    }

    fn owner(&self, p: usize) -> u8 {
        match self.color[p] {
            EMPTY => {
                let mut o = 0u8;
                for q in self.nbs(p) {
                    match self.color[q] {
                        BORDER => {}
                        EMPTY => return 0,
                        c if o == 0 => o = c,
                        c if c != o => return 0,
                        _ => {}
                    }
                }
                o
            }
            c => c,
        }
    }

    /// 트롬프-테일러식 간이 계가 (흑 - 백 - 덤)
    pub fn score(&self) -> f32 {
        let mut s = -self.komi;
        for y in 0..self.n {
            for x in 0..self.n {
                match self.owner(self.pt(x, y)) {
                    BLACK => s += 1.0,
                    WHITE => s -= 1.0,
                    _ => {}
                }
            }
        }
        s
    }

    /// MCTS 사전 지식: (가상 방문, 가상 승리)
    fn prior(&self, p: usize, c: u8) -> (f32, f32) {
        let (mut pn, mut pw) = (10.0f32, 5.0f32);
        let fill = 1.0 - self.empties.len() as f32 / (self.n * self.n) as f32;
        let mut tactical = false;
        for q in self.nbs(p) {
            let cq = self.color[q];
            if cq == 3 - c {
                match self.libs2(self.head[q]).0 {
                    1 => { pn += 20.0; pw += 20.0; tactical = true; } // 따내기
                    2 => { pn += 4.0; pw += 4.0; }                   // 단수 치기
                    _ => {}
                }
            } else if cq == c && self.libs2(self.head[q]).0 == 1 && self.libs_after(p, c) >= 2 {
                pn += 15.0; pw += 15.0; tactical = true; // 단수 살리기
            }
        }
        if self.libs_after(p, c) < 2 {
            pn += 20.0; // 자충 수
        }
        let (x, y) = self.xy(p);
        let d = x.min(y).min(self.n - 1 - x).min(self.n - 1 - y);
        if !tactical {
            match d {
                0 => pn += 15.0,
                1 if fill < 0.3 => pn += 6.0,
                2 | 3 if fill < 0.2 => { pn += 6.0; pw += 6.0; }
                _ => {}
            }
        }
        if self.last != PASS {
            let (lx, ly) = self.xy(self.last);
            if x.abs_diff(lx).max(y.abs_diff(ly)) <= 2 {
                pn += 4.0;
                pw += 4.0;
            }
        }
        (pn, pw)
    }
}

// ------------------------------------------------------------------ MCTS

#[derive(Clone)]
struct Node {
    mv: u16,
    nchild: u16,
    first_child: u32,
    n: f32,
    w: f32,
    an: f32,
    aw: f32,
    pn: f32,
    pw: f32,
}

struct Tree {
    nodes: Vec<Node>,
    root_color: u8,
    first_seen: Vec<usize>,
    moves: Vec<(usize, u8)>,
}

impl Tree {
    fn new(root: &Board) -> Self {
        Tree {
            nodes: vec![Node { mv: 0, nchild: 0, first_child: 0, n: 0.0, w: 0.0, an: 0.0, aw: 0.0, pn: 0.0, pw: 0.0 }],
            root_color: root.to_play,
            first_seen: vec![usize::MAX; root.w * root.w],
            moves: Vec::with_capacity(root.n * root.n * 3),
        }
    }

    fn expand(&mut self, ni: usize, b: &Board) {
        let c = b.to_play;
        let first = self.nodes.len() as u32;
        let mut count = 0u16;
        for &p in &b.empties {
            if p == b.ko || b.is_eye(p, c) || !b.is_legal(p, c) {
                continue;
            }
            let (pn, pw) = b.prior(p, c);
            self.nodes.push(Node { mv: p as u16, nchild: 0, first_child: 0, n: pn, w: pw, an: 0.0, aw: 0.0, pn, pw });
            count += 1;
        }
        // 패스: 판이 많이 비어 있으면 나쁜 수로 본다
        let fill = 1.0 - b.empties.len() as f32 / (b.n * b.n) as f32;
        let (pn, pw) = if fill < 0.6 && count > 0 { (40.0, 0.0) } else { (10.0, 5.0) };
        self.nodes.push(Node { mv: PASS as u16, nchild: 0, first_child: 0, n: pn, w: pw, an: 0.0, aw: 0.0, pn, pw });
        count += 1;
        self.nodes[ni].first_child = first;
        self.nodes[ni].nchild = count;
    }

    fn select(&self, ni: usize) -> usize {
        let node = &self.nodes[ni];
        let ln = node.n.max(1.0).ln();
        let mut best = (f32::MIN, 0usize);
        for ci in node.first_child as usize..(node.first_child as usize + node.nchild as usize) {
            let c = &self.nodes[ci];
            let q = c.w / c.n;
            let v = if c.an > 0.0 {
                let beta = c.an / (c.an + c.n + c.an * c.n / RAVE_EQ);
                (1.0 - beta) * q + beta * (c.aw / c.an)
            } else {
                q
            } + 0.05 * (ln / c.n).sqrt();
            if v > best.0 {
                best = (v, ci);
            }
        }
        best.1
    }

    fn iterate(&mut self, root: &Board, rng: &mut Rng) {
        let mut b = root.clone();
        let mut path: Vec<usize> = vec![0];
        self.moves.clear();
        let mut ni = 0;
        while b.passes < 2 {
            if self.nodes[ni].nchild == 0 {
                let visits = self.nodes[ni].n - self.nodes[ni].pn;
                if (ni == 0 || visits >= EXPAND_AT) && self.nodes.len() < MAX_NODES_PER_THREAD {
                    self.expand(ni, &b);
                } else {
                    break;
                }
            }
            let ci = self.select(ni);
            let mv = self.nodes[ci].mv as usize;
            self.moves.push((mv, b.to_play));
            b.play(mv);
            path.push(ci);
            ni = ci;
        }
        let mut moves = std::mem::take(&mut self.moves);
        if b.passes < 2 {
            b.playout(rng, &mut moves);
        }
        let black_win = b.score() > 0.0;

        // 역전파 + RAVE(AMAF)
        for f in self.first_seen.iter_mut() {
            *f = usize::MAX;
        }
        for (i, &(mv, _)) in moves.iter().enumerate() {
            if mv != PASS && self.first_seen[mv] == usize::MAX {
                self.first_seen[mv] = i;
            }
        }
        for (depth, &ni) in path.iter().enumerate() {
            self.nodes[ni].n += 1.0;
            if depth >= 1 && (moves[depth - 1].1 == BLACK) == black_win {
                self.nodes[ni].w += 1.0;
            }
            let pc = if depth % 2 == 0 { self.root_color } else { 3 - self.root_color };
            let win = (pc == BLACK) == black_win;
            let (fc, nc) = (self.nodes[ni].first_child as usize, self.nodes[ni].nchild as usize);
            for ci in fc..fc + nc {
                let m = self.nodes[ci].mv as usize;
                if m == PASS {
                    continue;
                }
                let f = self.first_seen[m];
                if f != usize::MAX && f >= depth && moves[f].1 == pc {
                    self.nodes[ci].an += 1.0;
                    if win {
                        self.nodes[ci].aw += 1.0;
                    }
                }
            }
        }
        self.moves = moves;
    }

    /// 루트 자식별 (좌표, 실제 방문, 실제 승리)
    fn root_stats(&self) -> Vec<(usize, f32, f32)> {
        let r = &self.nodes[0];
        (r.first_child as usize..r.first_child as usize + r.nchild as usize)
            .map(|ci| {
                let c = &self.nodes[ci];
                (c.mv as usize, c.n - c.pn, c.w - c.pw)
            })
            .collect()
    }

    /// 가장 많이 방문한 자식을 따라가는 예상 진행
    fn pv(&self, from_mv: usize, len: usize) -> Vec<usize> {
        let r = &self.nodes[0];
        let mut ni = match (r.first_child as usize..r.first_child as usize + r.nchild as usize)
            .find(|&ci| self.nodes[ci].mv as usize == from_mv)
        {
            Some(ci) => ci,
            None => return vec![],
        };
        let mut out = vec![];
        while out.len() < len && self.nodes[ni].nchild > 0 {
            let node = &self.nodes[ni];
            let best = (node.first_child as usize..node.first_child as usize + node.nchild as usize)
                .max_by(|&a, &b| (self.nodes[a].n - self.nodes[a].pn).total_cmp(&(self.nodes[b].n - self.nodes[b].pn)))
                .unwrap();
            if self.nodes[best].n - self.nodes[best].pn < 3.0 {
                break;
            }
            out.push(self.nodes[best].mv as usize);
            ni = best;
        }
        out
    }
}

// ------------------------------------------------------------------ 공개 API

#[derive(Serialize, Debug)]
pub struct GenMove {
    #[serde(rename = "move")]
    pub mv: String,
    pub winrate: f32,
    pub playouts: u64,
    pub pv: Vec<String>,
    pub comment: String,
}

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 8)
}

pub fn thread_count() -> usize {
    threads()
}

fn coord_of(b: &Board, p: usize) -> String {
    if p == PASS {
        "pass".into()
    } else {
        coord::format(b.to_idx(p), b.n)
    }
}

pub fn genmove(st: &GoState, ms: u64) -> GenMove {
    let root = Board::from_state(st);
    let deadline = Instant::now() + Duration::from_millis(ms.clamp(50, 60_000));
    let nthreads = threads();
    let seed = st.history.len() as u64 * 7919 + st.board.iter().map(|&c| c as u64).sum::<u64>();

    let results: Vec<(Vec<(usize, f32, f32)>, u64, Option<Tree>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..nthreads)
            .map(|t| {
                let root = &root;
                scope.spawn(move || {
                    let mut rng = Rng::new(seed.wrapping_add(t as u64 * 0x1234_5678_9ABC));
                    let mut tree = Tree::new(root);
                    let mut it = 0u64;
                    loop {
                        tree.iterate(root, &mut rng);
                        it += 1;
                        if it % 32 == 0 && Instant::now() >= deadline {
                            break;
                        }
                    }
                    let stats = tree.root_stats();
                    (stats, it, if t == 0 { Some(tree) } else { None })
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut agg: Vec<(usize, f32, f32)> = vec![];
    let mut total = 0u64;
    let mut tree0 = None;
    for (stats, it, tree) in results {
        total += it;
        if tree.is_some() {
            tree0 = tree;
        }
        for (mv, n, w) in stats {
            match agg.iter_mut().find(|e| e.0 == mv) {
                Some(e) => {
                    e.1 += n;
                    e.2 += w;
                }
                None => agg.push((mv, n, w)),
            }
        }
    }
    let best = agg.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((PASS, 0.0, 0.0));
    let winrate = if best.1 > 0.0 { best.2 / best.1 } else { 0.5 };
    let pv: Vec<String> = tree0
        .map(|t| t.pv(best.0, 6).into_iter().map(|p| coord_of(&root, p)).collect())
        .unwrap_or_default();
    let mv = coord_of(&root, best.0);
    let comment = describe(st, &root, best.0, winrate, &pv);
    GenMove { mv, winrate, playouts: total, pv, comment }
}

/// 수의 의도를 한국어로 설명한다
fn describe(st: &GoState, b: &Board, p: usize, winrate: f32, pv: &[String]) -> String {
    let wr = format!("예상 승률 {:.0}%", winrate * 100.0);
    let tail = if pv.is_empty() { String::new() } else { format!(", 예상 진행: {}", pv.join(" ")) };
    if p == PASS {
        return format!("더 둘 곳이 없다고 판단해 패스합니다. ({wr})");
    }
    let c = b.to_play;
    let mut reasons: Vec<String> = vec![];

    let mut after = st.clone();
    let captured = after.play_coord(&coord_of(b, p)).map(|c| c.len()).unwrap_or(0);
    if captured > 0 {
        reasons.push(format!("돌 {captured}개를 따냅니다"));
    }
    let saved = b.nbs(p).iter().any(|&q| b.color[q] == c && b.libs2(b.head[q]).0 == 1);
    if saved {
        reasons.push("단수에 몰린 돌을 살립니다".into());
    }
    let idx = b.to_idx(p);
    let atari = after.neighbors(idx).any(|q| after.board[q] == 3 - c && after.group(&after.board, q).1 == 1);
    if atari && captured == 0 {
        reasons.push("상대 돌을 단수로 몰아붙입니다".into());
    }
    if reasons.is_empty() {
        let n = b.n;
        let (x, y) = b.xy(p);
        let d = x.min(y).min(n - 1 - x).min(n - 1 - y);
        let stones = st.board.iter().filter(|&&s| s != EMPTY).count();
        let near_last = b.last != PASS && {
            let (lx, ly) = b.xy(b.last);
            x.abs_diff(lx).max(y.abs_diff(ly)) <= 1
        };
        let corner = x.min(n - 1 - x) <= n / 4 + 1 && y.min(n - 1 - y) <= n / 4 + 1;
        reasons.push(if near_last {
            "상대 수에 바로 응수합니다".into()
        } else if stones < n / 2 && corner && (d == 2 || d == 3) {
            "귀의 요처를 차지합니다".into()
        } else if d <= 1 {
            "변에서 집을 굳힙니다".into()
        } else if d <= 3 {
            "변과 귀의 세력을 넓힙니다".into()
        } else {
            "중앙으로 진출합니다".into()
        });
    }
    format!("{}. ({wr}{tail})", reasons.join(", "))
}

/// 현재 판에서 플레이아웃을 반복해 사석을 추정한다
pub fn dead_stones(st: &GoState, ms: u64) -> Vec<String> {
    let root = Board::from_state(st);
    let len = root.w * root.w;
    let deadline = Instant::now() + Duration::from_millis(ms.clamp(50, 30_000));
    let mut own = vec![0f32; len];
    let mut rng = Rng::new(0xDEAD_BEEF ^ st.history.len() as u64);
    let mut count = 0f32;
    let mut moves = Vec::new();
    while count < 4000.0 && (count < 200.0 || Instant::now() < deadline) {
        let mut b = root.clone();
        b.passes = 0;
        moves.clear();
        b.playout(&mut rng, &mut moves);
        for y in 0..b.n {
            for x in 0..b.n {
                let p = b.pt(x, y);
                match b.owner(p) {
                    BLACK => own[p] += 1.0,
                    WHITE => own[p] -= 1.0,
                    _ => {}
                }
            }
        }
        count += 1.0;
    }
    // 체인 단위로 평균 소유도를 보고, 상대 소유가 우세하면 사석
    let mut dead = vec![];
    let mut seen = vec![false; len];
    for y in 0..root.n {
        for x in 0..root.n {
            let p = root.pt(x, y);
            let c = root.color[p];
            if (c != BLACK && c != WHITE) || seen[p] {
                continue;
            }
            let h = root.head[p];
            let mut stones = vec![];
            let mut s = h;
            loop {
                seen[s] = true;
                stones.push(s);
                s = root.next[s];
                if s == h {
                    break;
                }
            }
            let avg = stones.iter().map(|&s| own[s]).sum::<f32>() / (stones.len() as f32 * count);
            let is_dead = if c == BLACK { avg < -0.2 } else { avg > 0.2 };
            if is_dead {
                dead.extend(stones.iter().map(|&s| coord::format(root.to_idx(s), root.n)));
            }
        }
    }
    dead.sort();
    dead
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(moves: &[&str], size: usize) -> GoState {
        let mut st = GoState::new(size, 6.5).unwrap();
        for m in moves {
            st.play_coord(m).unwrap();
        }
        st
    }

    /// 빠른 판의 규칙이 go.rs와 일치하는지 무작위 대국으로 확인
    #[test]
    fn fast_board_matches_reference() {
        let mut rng = Rng::new(42);
        for game in 0..30 {
            let mut st = GoState::new(9, 6.5).unwrap();
            for _ in 0..150 {
                let b = Board::from_state(&st);
                let p = b.policy_move(&mut rng);
                let mv = coord_of(&b, p);
                let mut fast = b.clone();
                fast.play(p);
                st.play_coord(&mv).unwrap_or_else(|e| panic!("game {game}: {e}"));
                let rebuilt = Board::from_state(&st);
                for y in 0..9 {
                    for x in 0..9 {
                        let q = fast.pt(x, y);
                        assert_eq!(fast.color[q], rebuilt.color[q]);
                    }
                }
                assert_eq!(fast.ko, rebuilt.ko, "ko mismatch");
                // 합법수 판정 일치
                for i in 0..81 {
                    if st.board[i] == EMPTY {
                        let q = rebuilt.pt(i % 9, i / 9);
                        assert_eq!(rebuilt.is_legal(q, rebuilt.to_play), st.is_legal(i), "legality mismatch at {i}");
                    }
                }
                if st.passes >= 2 {
                    break;
                }
            }
        }
    }

    #[test]
    fn captures_stone_in_atari() {
        // 백 E5가 단수 (흑 D5 F5 E4), 흑이 E6에 두면 따낸다
        let st = state(&["D5", "E5", "F5", "A1", "E4", "A2"], 9);
        let g = genmove(&st, 500);
        assert_eq!(g.mv, "E6", "{g:?}");
        assert!(g.comment.contains("따냅니다"));
    }

    #[test]
    fn saves_own_stone_in_atari() {
        // 흑 E5가 백 D5 F5 E4에 둘러싸여 단수 → 흑 E6으로 달아나야 함
        let st = state(&["E5", "D5", "A9", "F5", "J9", "E4"], 9);
        let g = genmove(&st, 800);
        assert_eq!(g.mv, "E6", "{g:?}");
    }

    #[test]
    #[ignore]
    fn bench() {
        for size in [9, 13, 19] {
            let st = state(&[], size);
            let t = Instant::now();
            let g = genmove(&st, 2000);
            println!("{size}x{size} 2s: {} playouts ({:.0}/s, {} threads) -> {} wr {:.2} pv {:?}",
                g.playouts, g.playouts as f64 / t.elapsed().as_secs_f64(), thread_count(), g.mv, g.winrate, g.pv);
        }
    }

    #[test]
    fn dead_stone_detection() {
        // 5x5: 흑이 B·D열 두 줄로 판을 장악, 백 A3 한 점은 흑 진영 안에 고립
        let mut st = GoState::new(5, 0.5).unwrap();
        let mut moves = vec!["B1", "A3"];
        for m in ["B2", "B3", "B4", "B5", "D1", "D2", "D3", "D4", "D5"] {
            moves.push(m);
            moves.push("pass");
        }
        for m in moves {
            st.play_coord(m).unwrap();
        }
        let dead = dead_stones(&st, 300);
        assert_eq!(dead, vec!["A3".to_string()]);
    }
}
