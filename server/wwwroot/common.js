// 공통 모듈: 서버 API, 좌표, 판 그리기, 키보드 커서, 사석함, 타이머, 음성 입출력, 게임 화면 틀
"use strict";

const COLS = "ABCDEFGHJKLMNOPQRST"; // I 제외 (바둑 표준 표기)
const NAME = { 1: "흑", 2: "백" };
const $ = id => document.getElementById(id);

const Common = {
  games: {},        // 탭 이름 → 게임 객체
  active: null,     // 지금 보고 있는 게임

  level() { return $("level").value; },

  async api(path, payload) {
    const res = await fetch(path, payload === undefined ? {} : {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    let data;
    try { data = await res.json(); } catch { data = {}; }
    if (res.status === 401 && !path.startsWith("/api/auth/")) {
      Common.showLogin("로그인이 필요합니다. 다시 로그인해 주세요.");
    }
    if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
    return data;
  },

  toCoord(x, y, size) { return COLS[x] + (size - y); },
  idxToCoord(i, size) { return Common.toCoord(i % size, Math.floor(i / size), size); },
  fromCoord(str, size) {
    const m = /^([A-HJ-T])(\d{1,2})$/i.exec(String(str).trim());
    if (!m) return null;
    const x = COLS.indexOf(m[1].toUpperCase());
    const y = size - parseInt(m[2], 10);
    if (x < 0 || x >= size || y < 0 || y >= size) return null;
    return { x, y };
  },

  escape(s) {
    return String(s).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  },

  /** 해설 목록에 한 줄 추가 + 음성 안내 */
  log(listId, html, isError = false) {
    const ol = $(listId);
    const li = document.createElement("li");
    li.innerHTML = html;
    if (isError) li.classList.add("err");
    ol.appendChild(li);
    ol.scrollTop = ol.scrollHeight;
    Common.speak(li.textContent);
  },

  /** 스크린리더 + 음성으로 즉시 알림 (해설 목록에는 남기지 않음) */
  announce(text) {
    const el = $("sr-announce");
    el.textContent = "";
    setTimeout(() => { el.textContent = text; }, 50);
    Common.speak(text);
  },

  describeBoard(board, size) {
    const groups = { 1: [], 2: [] };
    board.forEach((c, i) => { if (c) groups[c].push(Common.idxToCoord(i, size)); });
    const part = (name, list) => list.length ? `${name} ${list.length}개: ${list.join(", ")}` : `${name} 없음`;
    return `${part("흑", groups[1])}. ${part("백", groups[2])}.`;
  },
};

// ------------------------------------------------------------ 음성 출력

Common.lastSpoken = "";
Common.voiceOn = () => $("voice-out").checked;
Common.speak = text => {
  if (!text) return;
  Common.lastSpoken = text;
  if (!Common.voiceOn() || !("speechSynthesis" in window)) return;
  const u = new SpeechSynthesisUtterance(String(text).replace(/[—·]/g, ", "));
  u.lang = "ko-KR";
  u.rate = parseFloat($("voice-rate").value) || 1;
  const ko = speechSynthesis.getVoices().find(v => v.lang && v.lang.startsWith("ko"));
  if (ko) u.voice = ko;
  speechSynthesis.speak(u);
};
Common.stopSpeaking = () => { if ("speechSynthesis" in window) speechSynthesis.cancel(); };

// ------------------------------------------------------------ 음성 입력

/** 한국어로 말한 좌표·명령을 해석한다. 예: "디 사" → {move:"D4"}, "패스" → {cmd:"pass"} */
Common.parseSpeech = raw => {
  const t = String(raw).toLowerCase().replace(/[\s.,!?]/g, "");
  const commands = [
    [/^(패스|통과|pass)/, "pass"], [/^(무르기|물러|되돌|취소|undo)/, "undo"], [/^(판읽|판을읽|보드읽|읽어)/, "read"],
    [/^(새게임|새대국|새문제|다시시작|새로)/, "new"], [/^(힌트|도움)/, "hint"], [/^(기권|항복|포기)/, "resign"],
    [/^(남은시간|시간|몇초)/, "time"], [/^(다시말|반복|뭐라고|다시한번)/, "repeat"], [/^(다시풀|다시도전)/, "retry"],
    [/^(다음문제|다음)/, "next"], [/^(조용|그만말|멈춰)/, "quiet"],
    [/^(앱종료|앱끝내|프로그램종료|프로그램끝내|앱닫|나가기)/, "quit"],
    [/^(내기록|기록|전적)/, "records"], [/^(로그아웃)/, "logout"], [/^(이전|뒤로|앞수)/, "prev"],
    [/^(대국끝|대국종료|게임끝|게임종료|그만두|그만할|끝내기|끝내|계가|그만풀)/, "finish"],
  ];
  for (const [re, cmd] of commands) if (re.test(t)) return { cmd };

  // 알파벳의 한국어 발음 (긴 것부터 맞춘다)
  const letters = [
    ["에이치", "H"], ["에치", "H"], ["에이", "A"], ["에프", "F"], ["에스", "S"], ["제이", "J"], ["케이", "K"],
    ["엘", "L"], ["엠", "M"], ["엔", "N"], ["비", "B"], ["씨", "C"], ["시", "C"], ["디", "D"], ["이", "E"],
    ["지", "G"], ["쥐", "G"], ["오", "O"], ["피", "P"], ["큐", "Q"], ["알", "R"], ["티", "T"],
  ];
  let letter = null, rest = t;
  const m = /^([a-hj-t])(.*)$/.exec(t);
  if (m) { letter = m[1].toUpperCase(); rest = m[2]; }
  else {
    for (const [k, v] of letters) if (t.startsWith(k)) { letter = v; rest = t.slice(k.length); break; }
  }
  if (!letter) return null;
  rest = rest.replace(/^(열|에|의)/, "").replace(/(행|번|줄|에|요)$/, "");
  let num = null;
  if (/^\d{1,2}$/.test(rest)) num = parseInt(rest, 10);
  else {
    const digit = { 일: 1, 이: 2, 삼: 3, 사: 4, 오: 5, 육: 6, 륙: 6, 칠: 7, 팔: 8, 구: 9 };
    const k = /^(십)?([일이삼사오육륙칠팔구])?$/.exec(rest);
    if (k && (k[1] || k[2])) num = (k[1] ? 10 : 0) + (k[2] ? digit[k[2]] : 0);
  }
  if (!num) return null;
  return { move: letter + num };
};

Common.Recognizer = window.SpeechRecognition || window.webkitSpeechRecognition;
Common.listen = () => {
  if (!Common.Recognizer) {
    Common.announce("이 브라우저는 음성 인식을 지원하지 않습니다. 크롬이나 엣지를 사용해 주세요.");
    return;
  }
  const game = Common.active;
  Common.stopSpeaking();
  const rec = new Common.Recognizer();
  rec.lang = "ko-KR";
  rec.maxAlternatives = 5;
  rec.interimResults = false;
  const btn = $("mic");
  btn.classList.add("listening");
  btn.setAttribute("aria-pressed", "true");
  rec.onresult = e => {
    const alts = [...e.results[0]].map(a => a.transcript);
    const parsed = alts.map(Common.parseSpeech).find(Boolean);
    if (!parsed) { Common.announce(`"${alts[0]}"를 알아듣지 못했습니다. 예: 디 사, 패스, 무르기`); return; }
    if (parsed.cmd === "repeat") { Common.speak(Common.lastSpoken); return; }
    if (parsed.cmd === "quiet") { Common.stopSpeaking(); return; }
    if (parsed.cmd === "quit") {
      if ($("quit").hidden) { Common.announce("앱 종료는 서버가 있는 컴퓨터에서만 할 수 있습니다. 로그아웃하거나 창을 닫으세요."); return; }
      Common.quit(true); return;
    }
    if (parsed.cmd === "records") { showTab("records"); return; }
    if (parsed.cmd === "logout") { Common.logout(); return; }
    if (Common.onTabVoice && Common.onTabVoice(parsed)) return;
    if (!game) { Common.announce("먼저 게임을 고르세요."); return; }
    game.voice(parsed);
  };
  rec.onerror = e => {
    const unsupported = "이 브라우저에서는 음성 인식 서비스를 쓸 수 없습니다. VS Code 통합 브라우저라면 크롬이나 엣지에서 열어 주세요. 좌표 입력과 키보드로는 계속 둘 수 있습니다.";
    const msg = {
      "not-allowed": "마이크 권한이 없습니다. 브라우저에서 마이크를 허용해 주세요.",
      "service-not-allowed": unsupported,
      network: navigator.onLine ? unsupported : "음성 인식에 인터넷 연결이 필요합니다.",
      "no-speech": "말소리가 들리지 않았습니다.",
      "audio-capture": "마이크를 찾을 수 없습니다.",
    }[e.error];
    Common.announce(msg || `음성 인식 오류: ${e.error}`);
  };
  rec.onend = () => { btn.classList.remove("listening"); btn.setAttribute("aria-pressed", "false"); };
  rec.start();
};

// ------------------------------------------------------------ 로그인

Common.user = null;
Common.afterLogin = [];   // 로그인 직후 실행할 일 (사활 문제 목록 등)

Common.showLogin = msg => {
  Common.user = null;
  for (const g of Object.values(Common.games)) g.clock.stop();
  document.body.classList.add("logged-out");
  $("login-msg").textContent = msg || "";
  setTimeout(() => $("login-email").focus(), 0);
  Common.announce((msg ? msg + " " : "") + "바둑 게임방 로그인 화면입니다. 이메일과 비밀번호를 입력하세요. 처음이시면 회원가입을 누르세요.");
};

Common.onLogin = async user => {
  Common.user = user;
  document.body.classList.remove("logged-out");
  $("user-name").textContent = `${user.displayName}님`;
  Common.announce(`${user.displayName}님, 환영합니다.`);
  for (const f of Common.afterLogin) { try { await f(user); } catch {} }
  fetch("/api/status").then(r => r.json()).then(st => {
    $("status").textContent = `AI 준비됨 (${st.engine}, ${st.threads}스레드)`;
    $("status").className = "status ok";
  }).catch(() => { $("status").textContent = "서버 연결 실패"; $("status").className = "status err"; });
};

Common.logout = async () => {
  try { await Common.api("/api/auth/logout", {}); } catch {}
  location.reload();
};

function setupLogin() {
  const msg = $("login-msg");
  const submit = async (path, body) => {
    msg.textContent = "";
    try {
      const user = await Common.api(path, body);
      await Common.onLogin(user);
    } catch (e) {
      msg.textContent = e.message;
      Common.announce(e.message);
    }
  };
  $("login-form").addEventListener("submit", e => {
    e.preventDefault();
    submit("/api/auth/login", { email: $("login-email").value, password: $("login-password").value });
  });
  $("register-form").addEventListener("submit", e => {
    e.preventDefault();
    if ($("reg-password").value !== $("reg-password2").value) {
      msg.textContent = "비밀번호 확인이 맞지 않습니다.";
      Common.announce(msg.textContent);
      return;
    }
    submit("/api/auth/register", { email: $("reg-email").value, password: $("reg-password").value, displayName: $("reg-name").value });
  });
  const toggle = showRegister => {
    $("login-form").hidden = showRegister;
    $("register-form").hidden = !showRegister;
    $("to-register").hidden = showRegister;
    $("to-login").hidden = !showRegister;
    msg.textContent = "";
    setTimeout(() => (showRegister ? $("reg-email") : $("login-email")).focus(), 0);
    Common.announce(showRegister ? "회원가입 화면입니다. 이메일, 이름, 비밀번호를 입력하세요." : "로그인 화면입니다.");
  };
  $("to-register").addEventListener("click", () => toggle(true));
  $("to-login").addEventListener("click", () => toggle(false));
  $("logout").addEventListener("click", () => Common.logout());

  // Google 로그인: 서버에 클라이언트 ID가 설정된 경우에만 버튼을 보여 준다
  Common.api("/api/auth/config").then(cfg => {
    // 인터넷(터널)으로 접속한 화면에서는 "앱 종료"를 숨긴다 — 서버를 끌 수 있는 건 이 컴퓨터뿐
    if (!cfg.canShutdown) $("quit").hidden = true;
    if (!cfg.googleClientId) return;
    const s = document.createElement("script");
    s.src = "https://accounts.google.com/gsi/client";
    s.async = true;
    s.onload = () => {
      google.accounts.id.initialize({
        client_id: cfg.googleClientId,
        callback: r => submit("/api/auth/google", { credential: r.credential }),
      });
      google.accounts.id.renderButton($("google-login"), { theme: "outline", size: "large", text: "signin_with", locale: "ko" });
      $("google-wrap").hidden = false;
    };
    document.head.appendChild(s);
  }).catch(() => {});
}

// ------------------------------------------------------------ 앱 끝내기

/** 서버를 멈추고 화면을 종료 안내로 바꾼다. byVoice면 확인 창 없이 바로 (말로 요청한 경우) */
Common.quit = async byVoice => {
  if (!byVoice && !confirm("앱을 끝낼까요? 진행 중인 게임은 저장되지 않습니다.")) return;
  for (const g of Object.values(Common.games)) g.clock.stop();
  try {
    await Common.api("/api/shutdown", {});
  } catch (e) {
    Common.announce(`끝내지 못했습니다: ${e.message}`);
    return;
  }
  const msg = "앱을 종료했습니다. 이 창을 닫으셔도 됩니다. 다시 하려면 run.sh를 실행하세요.";
  document.body.innerHTML = `<main class="bye"><h1>바둑 게임방을 종료했습니다</h1><p>이 창을 닫으셔도 됩니다.</p><p>다시 하려면 터미널에서 <code>./run.sh</code>를 실행하세요.</p><div id="sr-announce" class="sr-only" aria-live="assertive"></div></main>`;
  Common.announce(msg);
  setTimeout(() => window.close(), 4000); // 스크립트로 연 창이면 닫힌다
};

// ------------------------------------------------------------ 판 그리기

Common.Grid = class {
  constructor(canvas, size) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.setSize(size);
  }
  setSize(size) {
    this.size = size;
    const W = this.canvas.width;
    this.margin = W * 0.06;
    this.step = (W - 2 * this.margin) / (size - 1);
  }
  pos(i) { return this.margin + i * this.step; }
  pick(evt) {
    const r = this.canvas.getBoundingClientRect();
    const sx = (evt.clientX - r.left) * (this.canvas.width / r.width);
    const sy = (evt.clientY - r.top) * (this.canvas.height / r.height);
    const x = Math.round((sx - this.margin) / this.step);
    const y = Math.round((sy - this.margin) / this.step);
    if (x < 0 || y < 0 || x >= this.size || y >= this.size) return null;
    return { x, y };
  }
  drawBoard(starPoints) {
    const { ctx, size } = this;
    const W = this.canvas.width;
    ctx.fillStyle = "#dcb35c";
    ctx.fillRect(0, 0, W, W);
    ctx.strokeStyle = "#3b2a14";
    ctx.lineWidth = 1.2;
    for (let i = 0; i < size; i++) {
      ctx.beginPath(); ctx.moveTo(this.pos(0), this.pos(i)); ctx.lineTo(this.pos(size - 1), this.pos(i)); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(this.pos(i), this.pos(0)); ctx.lineTo(this.pos(i), this.pos(size - 1)); ctx.stroke();
    }
    ctx.fillStyle = "#3b2a14";
    for (const [x, y] of starPoints) {
      ctx.beginPath(); ctx.arc(this.pos(x), this.pos(y), Math.max(3, this.step * 0.1), 0, Math.PI * 2); ctx.fill();
    }
    ctx.font = `${Math.round(this.margin * 0.38)}px sans-serif`;
    ctx.textAlign = "center"; ctx.textBaseline = "middle";
    for (let i = 0; i < size; i++) {
      ctx.fillText(COLS[i], this.pos(i), this.margin * 0.45);
      ctx.fillText(COLS[i], this.pos(i), W - this.margin * 0.45);
      ctx.fillText(String(size - i), this.margin * 0.45, this.pos(i));
      ctx.fillText(String(size - i), W - this.margin * 0.45, this.pos(i));
    }
  }
  drawStone(x, y, color, alpha = 1) {
    const { ctx } = this;
    const cx = this.pos(x), cy = this.pos(y), r = this.step * 0.47;
    ctx.save();
    ctx.globalAlpha = alpha;
    const g = ctx.createRadialGradient(cx - r * 0.35, cy - r * 0.35, r * 0.1, cx, cy, r);
    if (color === 1) { g.addColorStop(0, "#666"); g.addColorStop(1, "#111"); }
    else { g.addColorStop(0, "#fff"); g.addColorStop(1, "#cfcac0"); }
    ctx.fillStyle = g;
    ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI * 2); ctx.fill();
    ctx.strokeStyle = "rgba(0,0,0,.35)"; ctx.lineWidth = 1; ctx.stroke();
    ctx.restore();
  }
  drawMarker(x, y, stoneColor) {
    const { ctx } = this;
    ctx.strokeStyle = stoneColor === 1 ? "#fff" : "#000";
    ctx.lineWidth = 2;
    ctx.beginPath(); ctx.arc(this.pos(x), this.pos(y), this.step * 0.2, 0, Math.PI * 2); ctx.stroke();
  }
  drawCross(x, y) {
    const { ctx } = this;
    const cx = this.pos(x), cy = this.pos(y), d = this.step * 0.22;
    ctx.strokeStyle = "#c62828"; ctx.lineWidth = 2.5;
    ctx.beginPath(); ctx.moveTo(cx - d, cy - d); ctx.lineTo(cx + d, cy + d); ctx.moveTo(cx + d, cy - d); ctx.lineTo(cx - d, cy + d); ctx.stroke();
  }
  drawRing(x, y, color = "#c62828") {
    const { ctx } = this;
    ctx.strokeStyle = color; ctx.lineWidth = 4;
    ctx.beginPath(); ctx.arc(this.pos(x), this.pos(y), this.step * 0.3, 0, Math.PI * 2); ctx.stroke();
  }
  drawRegion(x0, y0, x1, y1) {
    const { ctx } = this, h = this.step / 2;
    ctx.save();
    ctx.strokeStyle = "rgba(21,101,192,.7)"; ctx.setLineDash([6, 4]); ctx.lineWidth = 2;
    ctx.strokeRect(this.pos(x0) - h, this.pos(y0) - h, this.pos(x1) - this.pos(x0) + 2 * h, this.pos(y1) - this.pos(y0) + 2 * h);
    ctx.restore();
  }
};

/** 키보드 커서: 판(canvas)에서 방향키로 이동하고 Enter/Space로 착수 */
Common.Cursor = class {
  constructor(canvas, opts) {
    this.opts = opts;
    this.canvas = canvas;
    this.x = null;
    this.y = null;
    canvas.addEventListener("focus", () => {
      if (this.x == null) { const c = Math.floor(opts.size() / 2); this.x = c; this.y = c; }
      this.say();
      opts.redraw();
    });
    canvas.addEventListener("blur", () => opts.redraw());
    canvas.addEventListener("keydown", e => {
      const n = opts.size();
      const moves = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
      if (moves[e.key]) {
        e.preventDefault();
        const [dx, dy] = moves[e.key];
        this.x = Math.min(n - 1, Math.max(0, (this.x ?? 0) + dx));
        this.y = Math.min(n - 1, Math.max(0, (this.y ?? 0) + dy));
        this.say();
        opts.redraw();
      } else if ((e.key === "Enter" || e.key === " ") && this.x != null) {
        e.preventDefault();
        opts.onPlay(this.x, this.y);
      }
    });
  }
  active() { return document.activeElement === this.canvas && this.x != null; }
  say() {
    Common.stopSpeaking();
    Common.announce(`${Common.toCoord(this.x, this.y, this.opts.size())} ${this.opts.cellText(this.x, this.y)}`);
  }
  draw(grid) {
    if (!this.active() || this.x >= grid.size || this.y >= grid.size) return;
    const ctx = grid.ctx, d = grid.step * 0.5;
    ctx.save();
    ctx.strokeStyle = "#1565c0"; ctx.lineWidth = 3;
    ctx.strokeRect(grid.pos(this.x) - d, grid.pos(this.y) - d, d * 2, d * 2);
    ctx.restore();
  }
};

// ------------------------------------------------------------ 사석함

/** 따낸 돌 표시. captures = [흑이 따낸 수, 백이 따낸 수] */
Common.renderTray = (el, captures) => {
  const row = (who, stoneColor, n) => {
    const shown = Math.min(n, 40);
    const dots = Array.from({ length: shown }, () => `<span class="tray-stone ${stoneColor === 1 ? "b" : "w"}"></span>`).join("");
    return `<div class="tray-row" role="img" aria-label="${who} 사석함: 따낸 돌 ${n}개"><span class="tray-label">${who} 사석함</span>` +
      `<span class="tray-stones">${dots}${n > shown ? ` +${n - shown}` : ""}</span><b>${n}</b></div>`;
  };
  el.innerHTML = row("흑", 2, captures[0]) + row("백", 1, captures[1]);
};

// ------------------------------------------------------------ 타이머

/**
 * 사람 차례에만 흐르는 시계. 설정 형식: "none" | "move:30" | "main:600:30:3" (본시간·초읽기 초·횟수)
 * 남은 시간을 음성으로 알리고, 다 되면 onTimeout을 부른다.
 */
Common.Clock = class {
  constructor(el, onTimeout) {
    this.el = el;
    this.onTimeout = onTimeout;
    this.timer = null;
    this.reset("none");
  }
  reset(spec) {
    this.stop();
    const [kind, a, b, c] = String(spec).split(":");
    this.kind = kind;
    this.perMove = kind === "move" ? +a : 0;
    this.main = kind === "main" ? +a : 0;
    this.byo = kind === "main" ? +b : 0;
    this.periods = kind === "main" ? +c : 0;
    this.left = kind === "move" ? this.perMove : this.main;
    this.inByo = false;
    this.render();
  }
  enabled() { return this.kind !== "none"; }
  start() {
    if (!this.enabled() || this.timer) return;
    // 한 수당 제한과 초읽기는 매 수 새로 시작
    if (this.kind === "move") this.left = this.perMove;
    if (this.inByo) this.left = this.byo;
    this.render();
    this.timer = setInterval(() => this.tick(), 1000);
  }
  stop() {
    clearInterval(this.timer);
    this.timer = null;
    this.render();
  }
  tick() {
    this.left -= 1;
    if (this.left <= 0) {
      if (this.kind === "main" && !this.inByo && this.periods > 0) {
        this.inByo = true;
        this.left = this.byo;
        Common.announce(`본시간이 끝났습니다. 초읽기 ${this.byo}초, ${this.periods}회 남았습니다.`);
      } else if (this.kind === "main" && this.inByo && this.periods > 1) {
        this.periods -= 1;
        this.left = this.byo;
        Common.announce(`초읽기 ${this.periods}회 남았습니다.`);
      } else {
        this.stop();
        this.onTimeout();
        return;
      }
    } else {
      const l = this.left;
      const countdown = this.inByo || this.kind === "move";
      if (l === 60 && this.kind === "main" && !this.inByo) Common.announce("1분 남았습니다.");
      else if (countdown && (l === 20 || l === 10)) Common.announce(`${l}초`);
      else if (countdown && l <= 5) Common.speak(String(l));
    }
    this.render();
  }
  text() {
    if (!this.enabled()) return "";
    const fmt = s => `${Math.floor(s / 60)}:${String(Math.max(0, s) % 60).padStart(2, "0")}`;
    if (this.kind === "move") return `남은 시간 ${this.left}초 (한 수당 ${this.perMove}초)`;
    return this.inByo ? `초읽기 ${this.left}초 · ${this.periods}회 남음` : `남은 시간 ${fmt(this.left)} · 초읽기 ${this.byo}초 ${this.periods}회`;
  }
  sayTime() { Common.announce(this.enabled() ? this.text() : "시간 제한이 없습니다."); }
  render() {
    if (!this.el) return;
    this.el.textContent = this.text();
    this.el.hidden = !this.enabled();
    this.el.classList.toggle("urgent", Boolean(this.enabled() && this.timer && this.left <= 10));
  }
};

// ------------------------------------------------------------ 게임 화면 틀

/**
 * 판 하나짜리 게임의 공통 화면과 흐름 (좌표 입력, 키보드, 마우스, 음성, 사석함, 타이머, 해설).
 * 게임별 차이는 cfg의 훅으로 넣는다.
 */
Common.createGame = cfg => {
  const sec = $("tab-" + cfg.key);
  const k = cfg.key;
  sec.innerHTML = `
    <h2 class="sr-only">${cfg.title}</h2>
    <div class="game">
      <div class="board-wrap">
        <canvas id="${k}-canvas" width="720" height="720" tabindex="0" role="application"
                aria-roledescription="${cfg.title} 판" aria-label="${cfg.title} 판. 방향키로 이동, Enter로 착수"></canvas>
      </div>
      <aside class="panel">
        <div class="controls">${cfg.settingsHtml || ""}<button id="${k}-new" class="primary">${cfg.newLabel || "새 게임"}</button></div>
        <form id="${k}-form" class="controls coord-form">
          <label>좌표 <input id="${k}-coord" autocomplete="off" placeholder="${cfg.hasPass ? "예: D4 또는 pass" : "예: H8"}"></label>
          <button type="submit">두기</button>
        </form>
        <div class="controls">
          ${cfg.hasPass ? `<button id="${k}-pass">패스</button>` : ""}
          ${cfg.hasRetry ? `<button id="${k}-retry">다시 풀기</button>` : `<button id="${k}-undo">무르기</button>`}
          ${cfg.hasResign ? `<button id="${k}-resign">기권</button>` : ""}
          ${cfg.hasHint ? `<button id="${k}-hint">힌트</button>` : ""}
          <button id="${k}-finish">${cfg.finishLabel || "대국 끝내기"}</button>
          <button id="${k}-read">판 읽기</button>
          ${cfg.extraButtonsHtml || ""}
        </div>
        <div id="${k}-clock" class="clock" hidden></div>
        <div id="${k}-info" class="info" aria-live="polite"></div>
        ${cfg.tray ? `<div id="${k}-tray" class="tray"></div>` : ""}
        ${cfg.extraPanelHtml || ""}
        <h3>해설</h3>
        <ol id="${k}-log" class="log" role="log" aria-live="polite"></ol>
      </aside>
    </div>`;

  const LOG = `${k}-log`;
  const canvas = $(`${k}-canvas`);
  const g = {
    cfg, view: null, busy: false, hover: null,
    grid: new Common.Grid(canvas, cfg.size(null)),
    log: (html, err) => Common.log(LOG, html, err),
  };
  const size = () => cfg.size(g.view);
  const hidden = () => Boolean(g.view && cfg.cellHidden && cfg.cellHidden(g));
  g.size = size;
  g.humanTurn = () => Boolean(g.view && !g.view.over && !g.busy && g.view.state.to_play === g.view.humanColor);
  g.board = () => (cfg.boardOf ? cfg.boardOf(g) : g.view.state.board);
  g.clock = new Common.Clock($(`${k}-clock`), async () => {
    g.log("<b>시간이 다 되었습니다.</b>");
    if (await g.call("timeout", {})) g.logResult();
  });

  g.cursor = new Common.Cursor(canvas, {
    size,
    redraw: () => g.draw(),
    onPlay: (x, y) => g.humanMove(Common.toCoord(x, y, size())),
    cellText: (x, y) => {
      if (!g.view || hidden()) return "";
      const i = y * size() + x;
      const c = g.board()[i];
      return (c ? `${NAME[c]} 돌` : "빈 점") + ((cfg.lastMoves ? cfg.lastMoves(g.view) : []).includes(i) ? ", 직전 수" : "");
    },
  });

  g.draw = () => {
    const n = size();
    g.grid.setSize(n);
    g.grid.drawBoard(cfg.stars(n));
    if (g.view) {
      const board = g.board();
      board.forEach((c, i) => { if (c) g.grid.drawStone(i % n, Math.floor(i / n), c); });
      if (!hidden()) {
        for (const i of (cfg.lastMoves ? cfg.lastMoves(g.view) : [])) if (board[i]) g.grid.drawMarker(i % n, Math.floor(i / n), board[i]);
      }
      if (cfg.drawExtra) cfg.drawExtra(g);
      if (g.hover && g.humanTurn() && !board[g.hover.y * n + g.hover.x]) g.grid.drawStone(g.hover.x, g.hover.y, g.view.humanColor, 0.4);
    }
    g.cursor.draw(g.grid);
  };

  g.render = thinking => {
    g.draw();
    const v = g.view, playing = g.humanTurn();
    $(`${k}-coord`).disabled = !playing;
    if (cfg.hasPass) $(`${k}-pass`).disabled = !playing;
    if (cfg.hasResign) $(`${k}-resign`).disabled = !playing;
    if (cfg.hasHint) $(`${k}-hint`).disabled = !playing;
    $(`${k}-finish`).disabled = !v || v.over || g.busy;
    if (!cfg.hasRetry) $(`${k}-undo`).disabled = !v || g.busy || !v.canUndo;
    $(`${k}-read`).disabled = !v || hidden();
    if (cfg.tray && v) Common.renderTray($(`${k}-tray`), cfg.tray(v));
    if (cfg.onRender) cfg.onRender(g);
    // 시계: 사람 차례에만 흐름
    if (v && playing && !thinking) g.clock.start(); else g.clock.stop();
    const info = $(`${k}-info`);
    if (!v) { info.textContent = cfg.emptyText || "새 게임을 눌러 시작하세요."; return; }
    let text = (cfg.info ? cfg.info(g) : `나: ${NAME[v.humanColor]}`) + "\n";
    if (v.result) text += `결과: ${v.result}`;
    else if (v.over) text += "종국 — 계가 중";
    else if (thinking) text += thinking;
    else text += playing ? (cfg.turnText ? cfg.turnText(g) : "내 차례입니다.") : "AI 차례입니다.";
    info.innerHTML = Common.escape(text).replace(/(중)$/, '<span class="thinking">$1</span>');
  };

  g.call = async (path, body, thinking) => {
    g.busy = true;
    g.render(thinking);
    try {
      g.view = await Common.api(`${cfg.api}/${g.view.id}/${path}`, body);
      return true;
    } catch (e) {
      g.log(Common.escape(e.message), true);
      return false;
    } finally {
      g.busy = false;
      g.render();
    }
  };

  g.logResult = () => { if (g.view.result) g.log(`<b>결과: ${Common.escape(g.view.result)}</b>`); };

  g.capText = list => (list && list.length ? ` (따낸 돌 ${list.length}개: ${list.join(", ")})` : "");
  g.logWarnings = () => {
    const v = g.view;
    for (const a of v.ataris || []) {
      const mine = a.color === v.humanColor;
      g.log(`주의 — ${mine ? "내" : "상대"} 돌 ${a.stones.join(", ")}이(가) 단수입니다. 남은 활로 ${a.liberty}.`);
    }
    for (const t of (v.ai && v.ai.threats) || []) g.log(`주의 — ${Common.escape(t)}`);
  };

  g.aiTurn = async () => {
    const v = g.view;
    if (v.over || v.state.to_play === v.humanColor) return;
    if (await g.call("ai", { level: Common.level() }, "AI가 생각하는 중")) {
      const ai = g.view.ai || {};
      if (ai.move) {
        const mv = ai.move.toLowerCase() === "pass" ? "패스" : ai.move;
        g.log(`<span class="mv">AI ${NAME[3 - g.view.humanColor]} ${Common.escape(mv)}</span>${Common.escape(g.capText(ai.captured))} ${Common.escape(ai.comment || "")}`);
      }
      g.logWarnings();
      g.logResult();
      if (cfg.afterAi) await cfg.afterAi(g);
    }
  };

  g.humanMove = async move => {
    if (!g.humanTurn()) return;
    if (await g.call("play", { move })) {
      const mv = move.toLowerCase() === "pass" ? "패스" : move.toUpperCase();
      g.log(`나 ${NAME[g.view.humanColor]} ${Common.escape(mv)}${Common.escape(g.capText(g.view.captured))}`);
      if (cfg.afterHuman) { await cfg.afterHuman(g); return; }
      g.logWarnings();
      g.logResult();
      if (g.view.over && cfg.afterAi) { await cfg.afterAi(g); return; }
      if (cfg.turnContinues && cfg.turnContinues(g)) return; // 육목: 내 차례 계속
      await g.aiTurn();
    }
  };

  g.start = async () => {
    if (g.busy) return;
    try {
      g.view = await Common.api(`${cfg.api}/new`, cfg.newPayload());
    } catch (e) {
      g.log(Common.escape(e.message), true);
      return;
    }
    $(LOG).innerHTML = "";
    g.clock.reset($("timer").value);
    g.log(Common.escape(cfg.intro ? cfg.intro(g) : `새 게임: 나는 ${NAME[g.view.humanColor]}입니다.`));
    g.render();
    await g.aiTurn();
  };

  /** 저장된 대국 이어하기 */
  g.resume = async view => {
    g.view = view;
    $(LOG).innerHTML = "";
    g.clock.reset($("timer").value);
    const h = view.state.history || [];
    g.log(Common.escape(`이어서 둡니다. 지금까지 ${h.length}수, 나는 ${NAME[view.humanColor]}입니다.` + (h.length ? ` 마지막 수: ${h[h.length - 1].replace(/^B /, "흑 ").replace(/^W /, "백 ").replace(/pass$/, "패스")}.` : "")));
    g.render();
    await g.aiTurn();
  };

  g.read = () => {
    if (!g.view || hidden()) { Common.announce("지금은 판을 읽을 수 없습니다."); return; }
    let text = Common.describeBoard(g.board(), size());
    if (cfg.tray) { const c = cfg.tray(g.view); text += ` 따낸 돌: 흑 ${c[0]}개, 백 ${c[1]}개.`; }
    if (cfg.readExtra) text += " " + cfg.readExtra(g);
    Common.announce(text);
  };

  /** 음성·좌표 입력 명령 처리 */
  g.voice = parsed => {
    if (cfg.onVoice && cfg.onVoice(g, parsed)) return;
    if (parsed.move) {
      if (!Common.fromCoord(parsed.move, size())) { Common.announce(`${parsed.move}는 판 밖입니다.`); return; }
      if (!g.humanTurn()) { Common.announce("지금은 내 차례가 아닙니다."); return; }
      g.humanMove(parsed.move);
      return;
    }
    const click = id => { const b = $(id); if (b && !b.disabled) b.click(); else Common.announce("지금은 할 수 없습니다."); };
    const actions = {
      pass: () => cfg.hasPass ? click(`${k}-pass`) : Common.announce("이 게임에는 패스가 없습니다."),
      undo: () => cfg.hasRetry ? click(`${k}-retry`) : click(`${k}-undo`),
      read: () => g.read(),
      new: () => click(`${k}-new`),
      hint: () => cfg.hasHint ? click(`${k}-hint`) : Common.announce("이 게임에는 힌트가 없습니다."),
      resign: () => cfg.hasResign ? click(`${k}-resign`) : Common.announce("이 게임에는 기권이 없습니다."),
      retry: () => cfg.hasRetry ? click(`${k}-retry`) : Common.announce("다시 풀기는 사활 문제에서만 됩니다."),
      time: () => g.clock.sayTime(),
      finish: () => click(`${k}-finish`),
      next: () => Common.announce("다음 문제는 사활 문제에서만 됩니다."),
    };
    (actions[parsed.cmd] || (() => {}))();
  };

  // 입력 연결
  canvas.addEventListener("pointermove", e => { g.hover = g.grid.pick(e); g.draw(); });
  canvas.addEventListener("pointerleave", () => { g.hover = null; g.draw(); });
  canvas.addEventListener("click", e => {
    const p = g.grid.pick(e);
    if (p && g.view) g.humanMove(Common.toCoord(p.x, p.y, size()));
  });
  $(`${k}-form`).addEventListener("submit", e => {
    e.preventDefault();
    const inp = $(`${k}-coord`);
    const v = inp.value.trim();
    if (!v) return;
    const parsed = Common.fromCoord(v, size()) ? { move: v.toUpperCase() } : Common.parseSpeech(v);
    if (!parsed) { g.log(`"${Common.escape(v)}"는 올바른 좌표가 아닙니다.`, true); return; }
    inp.value = "";
    g.voice(parsed);
    setTimeout(() => inp.focus(), 0);
  });
  $(`${k}-new`).addEventListener("click", () => g.start());
  if (cfg.hasPass) $(`${k}-pass`).addEventListener("click", () => g.humanMove("pass"));
  if (!cfg.hasRetry) $(`${k}-undo`).addEventListener("click", async () => { if (await g.call("undo", {})) g.log("한 수 물렀습니다."); });
  if (cfg.hasResign) $(`${k}-resign`).addEventListener("click", async () => {
    if (confirm("기권하시겠습니까?") && await g.call("resign", {})) g.logResult();
  });
  $(`${k}-read`).addEventListener("click", () => g.read());
  $(`${k}-finish`).addEventListener("click", async () => {
    if (!g.view || g.view.over) return;
    if (await g.call("finish", {}, cfg.finishing || "대국을 끝내는 중")) {
      if (g.view.ai && g.view.ai.comment) g.log(Common.escape(g.view.ai.comment));
      g.logResult();
    }
  });
  if (cfg.setup) cfg.setup(g);

  Common.games[k] = g;
  g.render();
  return g;
};

// ------------------------------------------------------------ 탭, 설정, 단축키

function showTab(name) {
  document.querySelectorAll(".tabs button").forEach(b => {
    b.classList.toggle("active", b.dataset.tab === name);
    if (b.dataset.tab === name) b.setAttribute("aria-current", "page"); else b.removeAttribute("aria-current");
  });
  document.querySelectorAll(".tab").forEach(s => s.classList.toggle("active", s.id === "tab-" + name));
  for (const g of Object.values(Common.games)) if (g !== Common.games[name]) g.clock.stop();
  Common.active = Common.games[name] || null;
  if (Common.active) Common.active.render();
  if (name === "records" && Common.onShowRecords) Common.onShowRecords();
  const label = document.querySelector(`.tabs button[data-tab="${name}"]`)?.textContent;
  if (label) Common.announce(`${label} 화면입니다.`);
}

document.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll(".tabs button").forEach(b => b.addEventListener("click", () => showTab(b.dataset.tab)));
  document.querySelectorAll("[data-goto]").forEach(b => b.addEventListener("click", () => showTab(b.dataset.goto)));

  // 설정 기억
  for (const id of ["level", "timer", "voice-out", "voice-rate"]) {
    const el = $(id);
    const key = "set-" + id;
    try {
      const v = localStorage.getItem(key);
      if (v != null) { if (el.type === "checkbox") el.checked = v === "1"; else el.value = v; }
    } catch {}
    el.addEventListener("change", () => {
      try { localStorage.setItem(key, el.type === "checkbox" ? (el.checked ? "1" : "0") : el.value); } catch {}
    });
  }
  $("voice-out").addEventListener("change", () => { if (!Common.voiceOn()) Common.stopSpeaking(); else Common.speak("음성 안내를 켰습니다."); });
  $("mic").addEventListener("click", () => Common.listen());
  $("quit").addEventListener("click", () => Common.quit(false));
  if (!Common.Recognizer) $("mic").title = "이 브라우저는 음성 인식을 지원하지 않습니다 (크롬·엣지 권장)";

  // 단축키: Alt+M 음성 입력, Alt+R 다시 듣기, Alt+T 남은 시간, Alt+Q 끝내기
  document.addEventListener("keydown", e => {
    if (!e.altKey) return;
    const key = e.key.toLowerCase();
    if (key === "m") { e.preventDefault(); Common.listen(); }
    else if (key === "r") { e.preventDefault(); Common.speak(Common.lastSpoken); }
    else if (key === "t" && Common.active) { e.preventDefault(); Common.active.clock.sayTime(); }
    else if (key === "q" && !$("quit").hidden) { e.preventDefault(); Common.quit(false); }
  });

  setupLogin();
  Common.api("/api/auth/me").then(user => Common.onLogin(user)).catch(() => Common.showLogin());
});
