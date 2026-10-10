// 내 기록: 계정에 저장된 대국 목록, 전적, 이어하기, 기보 다시 듣기
"use strict";
(() => {
  const OUTCOME = { win: "승", loss: "패", draw: "무", none: "승부 없음", playing: "진행 중", solved: "정답", failed: "실패" };
  const KIND_NAME = { Go: "바둑", AtariGo: "따내기 바둑", Tsumego: "사활 문제" };
  const LINE_NAME = { omok: "오목", pente: "펜테", connect6: "육목" };
  const gameName = r => (r.kind === "Line" ? LINE_NAME[r.variant] || "오목" : KIND_NAME[r.kind] || r.kind);
  const fmtDate = iso => {
    const d = new Date(iso);
    return `${d.getMonth() + 1}월 ${d.getDate()}일 ${d.getHours()}시 ${String(d.getMinutes()).padStart(2, "0")}분`;
  };

  const sec = $("tab-records");
  sec.innerHTML = `
    <h2 class="sr-only">내 기록</h2>
    <div class="records">
      <div class="panel">
        <div class="controls"><button id="rec-refresh">새로 고침</button></div>
        <h3>전적</h3>
        <table id="rec-stats" class="stats"></table>
        <h3>대국 목록</h3>
        <p class="hint">최근 대국부터 보여 줍니다. 진행 중인 대국은 이어서 둘 수 있습니다.</p>
        <ul id="rec-list" class="rec-list"></ul>
      </div>
      <div class="panel" id="replay" hidden>
        <h3 id="replay-title">기보 다시 듣기</h3>
        <canvas id="replay-canvas" width="720" height="720" tabindex="0" role="application"
                aria-roledescription="기보 판" aria-label="기보 판. 왼쪽 오른쪽 방향키로 한 수씩 이동"></canvas>
        <div class="controls">
          <button id="rp-first">처음</button><button id="rp-prev">이전 수</button>
          <button id="rp-next">다음 수</button><button id="rp-last">마지막</button>
          <button id="rp-read">판 읽기</button><button id="rp-close">닫기</button>
        </div>
        <div id="rp-info" class="info" aria-live="polite"></div>
      </div>
    </div>`;

  const grid = new Common.Grid($("replay-canvas"), 9);
  let replay = null; // { title, result, size, frames, i }

  async function load() {
    let list;
    try {
      list = await Common.api("/api/records/");
    } catch (e) {
      $("rec-list").innerHTML = `<li class="err">${Common.escape(e.message)}</li>`;
      return;
    }
    // 전적
    const stats = {};
    for (const r of list) {
      const name = gameName(r);
      const st = (stats[name] = stats[name] || { win: 0, loss: 0, draw: 0, none: 0, playing: 0, solved: 0, failed: 0 });
      st[r.outcome] = (st[r.outcome] || 0) + 1;
    }
    const names = Object.keys(stats);
    $("rec-stats").innerHTML = names.length === 0 ? "<tr><td>아직 기록이 없습니다.</td></tr>" :
      "<tr><th scope='col'>게임</th><th scope='col'>기록</th></tr>" + names.map(n => {
        const s = stats[n];
        const text = n === "사활 문제"
          ? `정답 ${s.solved}번, 실패 ${s.failed}번`
          : `${s.win}승 ${s.loss}패` + (s.draw ? ` ${s.draw}무` : "") + (s.playing ? `, 진행 중 ${s.playing}판` : "");
        return `<tr><th scope="row">${n}</th><td>${text}</td></tr>`;
      }).join("");

    // 목록
    $("rec-list").innerHTML = list.length === 0 ? "<li>아직 대국 기록이 없습니다. 게임을 시작해 보세요.</li>" : list.map(r => {
      const color = r.kind === "Tsumego" ? "" : ` · 나: ${NAME[r.humanColor]}`;
      const label = `${r.title} · ${OUTCOME[r.outcome] || r.outcome} · ${r.moves}수${color} · ${fmtDate(r.updated)}`;
      return `<li><span class="rec-label">${Common.escape(label)}</span>
        ${r.status === "playing" ? `<button data-resume="${r.id}">이어하기</button>` : ""}
        <button data-replay="${r.id}">기보 다시 듣기</button>
        <button data-delete="${r.id}" aria-label="${Common.escape(r.title)} 기록 삭제">삭제</button></li>`;
    }).join("");
  }

  function drawReplay() {
    if (!replay) return;
    const n = replay.size, f = replay.frames[replay.i];
    grid.setSize(n);
    grid.drawBoard(starPoints(n));
    f.board.forEach((c, i) => { if (c) grid.drawStone(i % n, Math.floor(i / n), c); });
    const p = f.move && Common.fromCoord(f.move, n);
    if (p && f.board[p.y * n + p.x]) grid.drawMarker(p.x, p.y, f.board[p.y * n + p.x]);
    $("rp-info").textContent = `${replay.i} / ${replay.frames.length - 1}수 · ${f.label}` + (replay.result ? ` · 결과: ${replay.result}` : "");
  }

  function step(to) {
    if (!replay) return;
    const last = replay.frames.length - 1;
    const i = Math.max(0, Math.min(last, to));
    if (i === replay.i && to !== replay.i) Common.announce(i === 0 ? "처음 국면입니다." : "마지막 수입니다.");
    replay.i = i;
    drawReplay();
    if (i === replay.i) {
      const f = replay.frames[i];
      const cap = f.captured && f.captured.length ? ` 돌 ${f.captured.length}개를 따냄: ${f.captured.join(", ")}.` : "";
      Common.stopSpeaking();
      Common.announce(`${f.label}.${cap}` + (i === last && replay.result ? ` 결과: ${replay.result}` : ""));
    }
  }

  async function openReplay(id) {
    try {
      replay = await Common.api(`/api/records/${id}/replay`);
    } catch (e) {
      Common.announce(e.message);
      return;
    }
    replay.i = 0;
    $("replay").hidden = false;
    $("replay-title").textContent = `기보 다시 듣기 — ${replay.title}`;
    drawReplay();
    $("replay-canvas").focus();
    Common.announce(`${replay.title} 기보입니다. 모두 ${replay.frames.length - 1}수. 오른쪽 방향키나 "다음"이라고 말하면 한 수씩 들려 드립니다.`);
  }

  $("rec-list").addEventListener("click", async e => {
    const b = e.target.closest("button");
    if (!b) return;
    if (b.dataset.replay) openReplay(b.dataset.replay);
    if (b.dataset.resume) {
      try {
        const r = await Common.api(`/api/records/${b.dataset.resume}/resume`, {});
        showTab(r.tab);
        await Common.games[r.tab].resume(r.view);
      } catch (err) {
        Common.announce(err.message);
      }
    }
    if (b.dataset.delete && confirm("이 기록을 삭제할까요? 되돌릴 수 없습니다.")) {
      try {
        await fetch(`/api/records/${b.dataset.delete}`, { method: "DELETE" });
        Common.announce("기록을 삭제했습니다.");
        load();
      } catch (err) {
        Common.announce(err.message);
      }
    }
  });
  $("rec-refresh").addEventListener("click", load);
  $("rp-first").addEventListener("click", () => step(0));
  $("rp-prev").addEventListener("click", () => step(replay.i - 1));
  $("rp-next").addEventListener("click", () => step(replay.i + 1));
  $("rp-last").addEventListener("click", () => step(replay.frames.length - 1));
  $("rp-read").addEventListener("click", () => { if (replay) Common.announce(Common.describeBoard(replay.frames[replay.i].board, replay.size)); });
  $("rp-close").addEventListener("click", () => { replay = null; $("replay").hidden = true; });
  $("replay-canvas").addEventListener("keydown", e => {
    if (!replay) return;
    const keys = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };
    if (keys[e.key]) { e.preventDefault(); step(replay.i + keys[e.key]); }
    else if (e.key === "Home") { e.preventDefault(); step(0); }
    else if (e.key === "End") { e.preventDefault(); step(replay.frames.length - 1); }
  });

  // 내 기록 화면에서의 음성 명령: "다음", "이전", "판 읽기", "처음"
  Common.onTabVoice = parsed => {
    if (!$("tab-records").classList.contains("active")) return false;
    if (!replay) { if (parsed.cmd) { Common.announce("기보 다시 듣기를 먼저 고르세요."); return true; } return false; }
    const actions = { next: () => step(replay.i + 1), prev: () => step(replay.i - 1), read: () => $("rp-read").click(), new: () => step(0) };
    if (actions[parsed.cmd]) { actions[parsed.cmd](); return true; }
    return false;
  };

  Common.onShowRecords = load;
  Common.afterLogin.push(load);
})();
