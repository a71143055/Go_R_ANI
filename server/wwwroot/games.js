// 게임별 설정: 공통 화면 틀(Common.createGame)에 게임마다 다른 부분만 넣는다.
"use strict";

function starPoints(n) {
  const s = {
    5: [[2, 2]],
    7: [[3, 3]],
    9: [[2, 2], [6, 2], [4, 4], [2, 6], [6, 6]],
    13: [[3, 3], [9, 3], [6, 6], [3, 9], [9, 9]],
    15: [[3, 3], [11, 3], [7, 7], [3, 11], [11, 11]],
    19: [[3, 3], [9, 3], [15, 3], [3, 9], [9, 9], [15, 9], [3, 15], [9, 15], [15, 15]],
  };
  return s[n] || [];
}
const colorSelect = id => `<label>내 돌 <select id="${id}"><option value="1">흑 (선)</option><option value="2">백 (후)</option></select></label>`;
const lastOf = v => (v.state.last == null ? [] : Array.isArray(v.state.last) ? v.state.last : [v.state.last]);

// ------------------------------------------------------------ 바둑 (맹기 포함)

Common.createGame({
  key: "go",
  title: "바둑",
  api: "/api/go",
  newLabel: "새 대국",
  finishLabel: "대국 끝내기 (계가)",
  finishing: "AI가 계가하는 중",
  hasPass: true,
  hasResign: true,
  size: v => (v ? v.state.size : parseInt($("go-size").value, 10)),
  stars: starPoints,
  settingsHtml: `
    <label>판 <select id="go-size"><option>9</option><option>13</option><option>19</option></select></label>
    ${colorSelect("go-color")}
    <label>덤 <input id="go-komi" type="number" step="0.5" value="6.5"></label>
    <label>모드 <select id="go-blind">
      <option value="off">일반</option>
      <option value="human">맹기 (수순 공개)</option>
      <option value="strict">완전 맹기 (수순도 숨김)</option>
    </select></label>`,
  extraButtonsHtml: `<button id="go-peek" hidden>엿보기 (누르고 있기)</button>`,
  extraPanelHtml: `<div id="go-moves-wrap" hidden><h3>수순 (맹기)</h3><div id="go-moves" class="moves"></div></div>`,
  newPayload: () => ({
    size: parseInt($("go-size").value, 10),
    komi: parseFloat($("go-komi").value) || 0,
    humanColor: parseInt($("go-color").value, 10),
    blind: $("go-blind").value,
  }),
  intro: g => {
    const v = g.view;
    const extra = {
      off: "",
      human: " 맹기 대국입니다. 판에 돌이 보이지 않고, 직전 수만 1.5초 동안 표시됩니다.",
      strict: " 완전 맹기 대국입니다. 돌과 수순이 모두 숨겨지고 직전 수의 좌표만 알려 줍니다.",
    }[v.blind];
    return `새 대국: ${v.state.size}줄, 나는 ${NAME[v.humanColor]}입니다.${extra}`;
  },
  tray: v => v.state.captures,
  cellHidden: g => g.view.hidden && !g.peekBoard,
  boardOf: g => g.peekBoard || g.view.state.board,
  lastMoves: lastOf,
  info: g => {
    const v = g.view;
    const mode = { off: "", human: " · 맹기", strict: " · 완전 맹기" }[v.blind];
    return `${v.state.size}줄 · 나: ${NAME[v.humanColor]}${mode}` + (v.blind !== "off" ? ` · 엿보기 ${v.peeks}회` : "");
  },
  drawExtra: g => {
    const v = g.view, n = v.state.size, grid = g.grid;
    const score = v.score;
    if (score) {
      const dead = new Set((score.dead || []).map(c => { const p = Common.fromCoord(c, n); return p ? p.y * n + p.x : -1; }));
      dead.forEach(i => { if (i >= 0) { grid.drawStone(i % n, Math.floor(i / n), v.state.board[i], 0.45); grid.drawCross(i % n, Math.floor(i / n)); } });
      const ctx = grid.ctx, d = grid.step * 0.18;
      score.ownership.forEach((o, i) => {
        if (!o || (v.state.board[i] && !dead.has(i))) return;
        ctx.fillStyle = o === 1 ? "rgba(0,0,0,.75)" : "rgba(255,255,255,.9)";
        ctx.fillRect(grid.pos(i % n) - d, grid.pos(Math.floor(i / n)) - d, d * 2, d * 2);
      });
    }
    // 맹기(수순 공개): 직전 수를 잠깐 보여 준다
    if (v.hidden && !g.peekBoard && v.blind === "human" && v.state.last != null && Date.now() < (g.flashUntil || 0)) {
      const h = v.state.history[v.state.history.length - 1];
      const c = h && h[0] === "B" ? 1 : 2;
      grid.drawStone(v.state.last % n, Math.floor(v.state.last / n), c, 0.9);
    }
  },
  onRender: g => {
    const v = g.view;
    $("go-peek").hidden = !v || v.blind === "off" || v.over;
    $("go-moves-wrap").hidden = !v || v.blind !== "human";
    if (v && v.blind === "human") {
      const h = v.state.history;
      $("go-moves").innerHTML = h.map((m, i) => `<span class="${i === h.length - 1 ? "last" : ""}">${i + 1}.${Common.escape(m)}</span>`).join(" ") || "(아직 없음)";
      $("go-moves").scrollTop = $("go-moves").scrollHeight;
    }
    // 직전 수 깜빡 표시 시작
    if (v && v.hidden && v.blind === "human" && v.state.history.length !== g.lastHistLen) {
      g.lastHistLen = v.state.history.length;
      g.flashUntil = Date.now() + 1500;
      setTimeout(() => g.draw(), 1600);
    }
  },
  afterAi: async g => {
    if (g.view.over && !g.view.result) {
      if (await g.call("score", {}, "AI가 계가하는 중")) {
        if (g.view.ai) g.log(Common.escape(g.view.ai.comment));
        g.logResult();
      }
    }
  },
  setup: g => {
    // 맹기 엿보기: 누르고 있는 동안만 실제 판을 보여 준다
    let peeking = false;
    $("go-peek").addEventListener("pointerdown", async e => {
      e.preventDefault();
      if (!g.view || g.busy) return;
      peeking = true;
      try {
        const r = await Common.api(`/api/go/${g.view.id}/peek`, {});
        g.view.peeks = r.peeks;
        if (peeking) g.peekBoard = r.board; // 응답 전에 손을 뗐으면 보여 주지 않음
        g.render();
      } catch (err) {
        g.log(Common.escape(err.message), true);
      }
    });
    const end = () => { peeking = false; if (g.peekBoard) { g.peekBoard = null; g.render(); } };
    ["pointerup", "pointerleave", "pointercancel"].forEach(ev => $("go-peek").addEventListener(ev, end));
    $("go-size").addEventListener("change", () => { if (!g.view) g.draw(); });
  },
});

// ------------------------------------------------------------ 따내기 바둑

Common.createGame({
  key: "atari",
  title: "따내기 바둑",
  api: "/api/atari",
  hasPass: true,
  size: v => (v ? v.state.size : parseInt($("atari-size").value, 10)),
  stars: starPoints,
  settingsHtml: `
    <label>판 <select id="atari-size"><option>5</option><option selected>7</option><option>9</option></select></label>
    ${colorSelect("atari-color")}
    <label><input id="atari-cross" type="checkbox" checked> 십자 배치로 시작</label>`,
  newPayload: () => ({
    size: parseInt($("atari-size").value, 10),
    humanColor: parseInt($("atari-color").value, 10),
    cross: $("atari-cross").checked,
  }),
  intro: g => {
    const v = g.view;
    const start = v.state.board.some(c => c) ? ` 가운데에 흑백 두 점씩 놓고 시작합니다. ${Common.describeBoard(v.state.board, v.state.size)}` : "";
    return `따내기 바둑: ${v.state.size}줄, 나는 ${NAME[v.humanColor]}입니다. 돌을 하나라도 먼저 따내면 이깁니다.${start}`;
  },
  tray: v => v.state.captures,
  lastMoves: lastOf,
  info: g => `따내기 바둑 · ${g.view.state.size}줄 · 나: ${NAME[g.view.humanColor]} · 먼저 따내면 승리`,
  setup: g => $("atari-size").addEventListener("change", () => { if (!g.view) g.draw(); }),
});

// ------------------------------------------------------------ 사활 문제

Common.createGame({
  key: "tsumego",
  title: "사활 문제",
  api: "/api/tsumego",
  newLabel: "문제 시작",
  finishLabel: "그만 풀기 (정답 보기)",
  finishing: "정답을 찾는 중",
  hasRetry: true,
  hasHint: true,
  emptyText: "문제를 고르고 '문제 시작'을 누르세요.",
  size: v => (v ? v.state.size : 9),
  stars: starPoints,
  settingsHtml: `<label>문제 <select id="tsumego-problem"></select></label>`,
  extraButtonsHtml: `<button id="tsumego-next">다음 문제</button>`,
  newPayload: () => ({ id: $("tsumego-problem").value }),
  intro: g => {
    const p = g.view.meta.problem;
    return `${p.title} (${p.level}). ${p.goal_text}. ${p.description} 영역은 ${p.region[0]}부터 ${p.region[1]}까지입니다. ` +
      Common.describeBoard(g.view.state.board, g.view.state.size);
  },
  lastMoves: lastOf,
  info: g => {
    const p = g.view.meta.problem;
    return `${p.title} (${p.level}) · ${p.goal_text}`;
  },
  turnText: () => "흑 차례입니다. 정답을 찾아 두세요.",
  readExtra: g => `목표: ${g.view.meta.problem.goal_text}.`,
  drawExtra: g => {
    const p = g.view.meta.problem, n = g.view.state.size;
    const a = Common.fromCoord(p.region[0], n), b = Common.fromCoord(p.region[1], n);
    if (a && b) g.grid.drawRegion(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.max(a.x, b.x), Math.max(a.y, b.y));
  },
  // 사활 문제는 내 수와 백의 응수가 한 번에 처리된다
  afterHuman: async g => {
    const ai = g.view.ai || {};
    if (ai.move) g.log(`<span class="mv">백 ${Common.escape(ai.move === "PASS" ? "패스" : ai.move)}</span>${Common.escape(g.capText(ai.captured))}`);
    g.log(Common.escape(ai.comment || ""));
    if (g.view.meta.status === "solved") {
      g.log("<b>🎉 정답입니다!</b> '다음 문제'로 넘어가세요.");
      g.loadProblems();
    }
  },
  onVoice: (g, parsed) => {
    if (parsed.cmd === "next") { $("tsumego-next").click(); return true; }
    return false;
  },
  setup: async g => {
    const sel = $("tsumego-problem");
    // 문제 목록 (이 계정이 푼 문제는 ✓) — 로그인 뒤, 그리고 문제를 풀 때마다 갱신
    g.loadProblems = async () => {
      const keep = sel.value;
      const list = await Common.api("/api/tsumego/list");
      sel.innerHTML = list.map(p => `<option value="${p.id}">${Common.escape(`${p.solved ? "✓ " : ""}[${p.level}] ${p.title}`)}${p.solved ? " (푼 문제)" : ""}</option>`).join("");
      if (keep) sel.value = keep;
    };
    Common.afterLogin.push(() => g.loadProblems());
    $("tsumego-retry").addEventListener("click", async () => {
      if (!g.view) return;
      if (await g.call("retry", {})) g.log("처음 배치로 되돌렸습니다. " + Common.describeBoard(g.view.state.board, g.view.state.size));
    });
    $("tsumego-hint").addEventListener("click", async () => {
      if (!g.view) return;
      try {
        const r = await Common.api(`/api/tsumego/${g.view.id}/hint`, {});
        g.log(r.moves.length ? `힌트: 정답 수는 ${r.moves.join(", ")}입니다.` : "힌트: 이 국면에서는 정답이 없습니다. 다시 풀어 보세요.");
      } catch (e) {
        g.log(Common.escape(e.message), true);
      }
    });
    $("tsumego-next").addEventListener("click", () => {
      const i = sel.selectedIndex;
      sel.selectedIndex = (i + 1) % sel.options.length;
      g.start();
    });
  },
});

// ------------------------------------------------------------ 오목 · 펜테 · 육목

const LINE_RULES = {
  omok: { title: "오목", rule: "가로, 세로, 대각선으로 5개를 먼저 이으면 이깁니다." },
  pente: { title: "펜테", rule: "5개를 먼저 이으면 이깁니다. 상대 돌 2개를 양쪽에서 막으면 따내고, 10개를 먼저 따내도 이깁니다." },
  connect6: { title: "육목", rule: "6개를 먼저 이으면 이깁니다. 흑은 첫 수만 1개, 그 다음부터는 매 차례 2개씩 놓습니다." },
};
const lineQuota = st => (st.variant === "connect6" && st.history.length - st.placed_in_turn > 0 ? 2 : 1);

for (const variant of Object.keys(LINE_RULES)) {
  const r = LINE_RULES[variant];
  Common.createGame({
    key: variant,
    title: r.title,
    api: "/api/line",
    size: v => (v ? v.state.size : variant === "omok" ? 15 : 19),
    stars: starPoints,
    settingsHtml: colorSelect(`${variant}-color`),
    newPayload: () => ({ variant, humanColor: parseInt($(`${variant}-color`).value, 10) }),
    intro: g => `${r.title}: 나는 ${NAME[g.view.humanColor]}입니다. ${r.rule}`,
    tray: variant === "pente" ? v => v.state.captures : null,
    lastMoves: lastOf,
    info: g => `${r.title} · ${g.view.state.size}줄 · 나: ${NAME[g.view.humanColor]} · ${g.view.state.history.length}수`,
    turnText: g => {
      if (variant !== "connect6") return "내 차례입니다.";
      const st = g.view.state, left = lineQuota(st) - st.placed_in_turn;
      return `내 차례입니다. 돌을 ${left}개 더 놓으세요.`;
    },
    turnContinues: g => {
      const v = g.view;
      if (!v.over && v.state.to_play === v.humanColor) {
        Common.announce("한 개 더 놓으세요.");
        return true;
      }
      return false;
    },
    drawExtra: g => {
      const st = g.view.state, n = st.size;
      for (const i of st.win_line) g.grid.drawRing(i % n, Math.floor(i / n));
    },
    readExtra: () => r.rule,
  });
}
