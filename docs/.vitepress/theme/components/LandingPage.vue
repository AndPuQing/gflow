<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

const props = defineProps<{
    locale?: "en" | "zh-CN";
}>();

/*
 * The terminal session is a fixed, bilingual-agnostic CLI demo. Each row
 * reveals on its own frame; the job row flips PENDING -> RUNNING; a block
 * cursor sits on the most recent command. The whole thing loops.
 */
type TermRow = {
    kind: "cmd" | "ok" | "thead" | "trow" | "metric";
    at: number;
    text?: string;
    cells?: string[];
    id?: string;
    name?: string;
    gpu?: string;
    node?: string;
};

const TERMINAL: TermRow[] = [
    { kind: "cmd", text: "gflowd up", at: 1 },
    { kind: "ok", text: "daemon ready on 127.0.0.1:5577", at: 2 },
    { kind: "cmd", text: "gbatch --gpus 1 --project vision python train.py", at: 3 },
    { kind: "ok", text: "submitted batch job 184", at: 4 },
    { kind: "cmd", text: "gqueue", at: 5 },
    { kind: "thead", cells: ["JOBID", "NAME", "STATE", "GPU", "NODE"], at: 6 },
    { kind: "trow", id: "184", name: "train", gpu: "1", node: "e40a", at: 6 },
    { kind: "cmd", text: "gjob log 184", at: 8 },
    { kind: "metric", text: "step=420  loss=0.184  throughput=178 img/s", at: 9 },
];

const MAX_FRAME = 9;
const HOLD = 4;
const BEAT_MS = 780;
const JOB_RUNS_AT = 8;

const frame = ref(MAX_FRAME);
let timer: number | undefined;

onMounted(() => {
    const reduce =
        typeof window !== "undefined" &&
        window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (reduce) return;
    timer = window.setInterval(() => {
        frame.value += 1;
        if (frame.value > MAX_FRAME + HOLD) frame.value = 1;
    }, BEAT_MS);
});

onBeforeUnmount(() => {
    if (timer !== undefined) window.clearInterval(timer);
});

const activeCmd = computed(() => {
    let idx = -1;
    TERMINAL.forEach((row, i) => {
        if (row.kind === "cmd" && row.at <= frame.value) idx = i;
    });
    return idx;
});

const copies = {
    en: {
        hero: {
            eyebrow: "Single-node scheduling for shared machines",
            title: "Give one Linux machine a real job scheduler",
            lead: "A small CLI and a local daemon to submit, schedule, and inspect GPU or CPU jobs.",
            actions: [
                { label: "Quick Start", href: "/getting-started/quick-start" },
                { label: "Installation", href: "/getting-started/installation" },
                { label: "Command Reference", href: "/reference/quick-reference" },
            ],
            trust: [
                "Single-node by design",
                "GPU-aware scheduling",
                "tmux-backed execution",
                "MCP-ready for agents",
            ],
        },
        panel: { title: "Workstation session" },
        problem: {
            eyebrow: "Why it exists",
            title: "When one machine is no longer just yours",
            lead: "Ad-hoc tmux and GPU etiquette fall apart once the box is shared.",
            painTitle: "Without discipline",
            painItems: [
                "Long jobs collide with interactive work.",
                "Failures scatter across logs with no state to recover.",
                "GPU rules live in tribal knowledge, not policy.",
            ],
            valueTitle: "With gflow",
            valueItems: [
                "One daemon owns state and the queue.",
                "Every job is inspectable and recoverable.",
                "Resources and dependencies are declared up front.",
            ],
        },
        workflow: {
            eyebrow: "How it works",
            title: "Four commands, one controlled run",
            steps: [
                { idx: "01", cmd: "gflowd up", label: "Start the daemon" },
                { idx: "02", cmd: "gbatch --gpus 1", label: "Submit a job" },
                { idx: "03", cmd: "gqueue", label: "Watch it schedule" },
                { idx: "04", cmd: "gjob log <id>", label: "Follow the run" },
            ],
        },
        capabilities: {
            eyebrow: "Capabilities",
            title: "Built for daily workstation use",
            items: [
                { title: "Queueing & lifecycle", body: "Submit, hold, cancel, update, and redo jobs." },
                { title: "GPU-aware scheduling", body: "Request GPUs, share them, and cap VRAM." },
                { title: "Workflow composition", body: "Chain dependencies, arrays, and sweeps." },
                { title: "Operational visibility", body: "Read state as table, tree, JSON, CSV, or YAML." },
                { title: "Recoverable execution", body: "Every job runs in its own tmux session." },
                { title: "Automation & AI", body: "Drive the scheduler through a local MCP server." },
            ],
        },
        scenarios: {
            eyebrow: "Where it fits",
            title: "Where it fits",
            items: [
                { title: "Shared lab GPU server", body: "Coordinate many researchers on one box." },
                { title: "Solo research machine", body: "Keep long experiments structured and restartable." },
                { title: "Local eval pipelines", body: "Chain prep, train, benchmark, and report." },
            ],
        },
        pathways: {
            eyebrow: "Documentation",
            title: "Start where you are",
            items: [
                { title: "Install and launch", body: "Install, configure, and start the daemon.", href: "/getting-started/installation", cta: "Installation" },
                { title: "First workflow", body: "Submit a job, inspect the queue, read logs.", href: "/getting-started/quick-start", cta: "Quick Start" },
                { title: "Command reference", body: "Cheat sheet for gflowd, gbatch, gqueue, gjob, gctl.", href: "/reference/quick-reference", cta: "Reference" },
                { title: "Connect agents", body: "Run gflow as a local MCP server.", href: "/ai-integration/mcp-and-skills", cta: "AI Integration" },
            ],
        },
        mcp: {
            eyebrow: "AI integration",
            title: "Turn scheduler operations into agent tools",
            lead: "Run gflow as a local stdio MCP server; agent CLIs can inspect queues and drive workflows.",
            command: "gflow mcp serve",
            href: "/ai-integration/mcp-and-skills",
            cta: "Read the AI guide",
        },
        cta: {
            title: "Start scheduling the machine you already have",
            lead: "The docs double as an operator's handbook.",
            actions: [
                { label: "Install gflow", href: "/getting-started/installation" },
                { label: "Read Quick Start", href: "/getting-started/quick-start" },
            ],
        },
    },
    "zh-CN": {
        hero: {
            eyebrow: "面向共享机器的单节点调度",
            title: "让一台 Linux 机器拥有真正的任务调度器",
            lead: "一个轻量 CLI 加本地 daemon，完成 GPU / CPU 任务的提交、调度与查看。",
            actions: [
                { label: "快速开始", href: "/zh-CN/getting-started/quick-start" },
                { label: "安装指南", href: "/zh-CN/getting-started/installation" },
                { label: "命令速查", href: "/zh-CN/reference/quick-reference" },
            ],
            trust: [
                "单节点设计",
                "GPU 感知调度",
                "基于 tmux 的执行",
                "可供 Agent 使用的 MCP",
            ],
        },
        panel: { title: "工作站会话" },
        problem: {
            eyebrow: "为什么需要它",
            title: "当一台机器不再只属于你",
            lead: "一旦机器开始共享，临时 tmux 和口头约定就会失效。",
            painTitle: "没有纪律时",
            painItems: [
                "长任务和交互式工作互相打架。",
                "失败后日志散落，没有状态可恢复。",
                "GPU 规则靠经验，而不是策略。",
            ],
            valueTitle: "用 gflow 后",
            valueItems: [
                "一个 daemon 统一管理状态与队列。",
                "每个任务可查看、可恢复。",
                "资源与依赖在提交前明确声明。",
            ],
        },
        workflow: {
            eyebrow: "工作流",
            title: "四条命令，一次可控运行",
            steps: [
                { idx: "01", cmd: "gflowd up", label: "启动 daemon" },
                { idx: "02", cmd: "gbatch --gpus 1", label: "提交任务" },
                { idx: "03", cmd: "gqueue", label: "查看调度" },
                { idx: "04", cmd: "gjob log <id>", label: "跟踪日志" },
            ],
        },
        capabilities: {
            eyebrow: "能力概览",
            title: "为日常工作站而设计",
            items: [
                { title: "队列与生命周期", body: "提交、挂起、取消、更新与重做任务。" },
                { title: "GPU 感知调度", body: "声明 GPU、共享、并限制显存。" },
                { title: "工作流编排", body: "用依赖、数组任务与参数扫描串联。" },
                { title: "可观测性", body: "以表格、树、JSON、CSV 或 YAML 查看状态。" },
                { title: "可恢复执行", body: "每个任务都跑在独立 tmux 会话里。" },
                { title: "自动化与 AI", body: "通过本地 MCP server 驱动调度。" },
            ],
        },
        scenarios: {
            eyebrow: "适用场景",
            title: "适用场景",
            items: [
                { title: "共享实验室 GPU 服务器", body: "多人共用一台机器，用规则替代口头协调。" },
                { title: "个人研究主机", body: "让长时间实验保持结构化、可恢复。" },
                { title: "本地评测流水线", body: "串联预处理、训练、评测与汇总。" },
            ],
        },
        pathways: {
            eyebrow: "文档",
            title: "从你的阶段开始",
            items: [
                { title: "安装并启动", body: "安装、配置并启动 daemon。", href: "/zh-CN/getting-started/installation", cta: "安装指南" },
                { title: "第一个流程", body: "提交任务、查看队列、读取日志。", href: "/zh-CN/getting-started/quick-start", cta: "快速开始" },
                { title: "命令速查", body: "gflowd、gbatch、gqueue、gjob、gctl 速查。", href: "/zh-CN/reference/quick-reference", cta: "命令速查" },
                { title: "连接 Agent", body: "把 gflow 作为本地 MCP server 运行。", href: "/zh-CN/ai-integration/mcp-and-skills", cta: "AI 集成" },
            ],
        },
        mcp: {
            eyebrow: "AI 集成",
            title: "把调度操作变成 Agent 工具",
            lead: "将 gflow 作为本地 stdio MCP server 运行，Agent CLI 可查看队列并驱动流程。",
            command: "gflow mcp serve",
            href: "/zh-CN/ai-integration/mcp-and-skills",
            cta: "阅读 AI 指南",
        },
        cta: {
            title: "从你已有的那台机器开始调度",
            lead: "文档同时也是一本运维手册。",
            actions: [
                { label: "安装 gflow", href: "/zh-CN/getting-started/installation" },
                { label: "阅读快速开始", href: "/zh-CN/getting-started/quick-start" },
            ],
        },
    },
};

const currentLocale = computed(() => (props.locale === "zh-CN" ? "zh-CN" : "en"));
const copy = computed(() => copies[currentLocale.value]);

function pad(n: number) {
    return String(n).padStart(2, "0");
}
</script>

<template>
    <div class="landing-page">
        <!-- HERO -->
        <header class="lp-hero">
            <div class="lp-container">
                <p class="lp-label"><span class="lp-label-tick" aria-hidden="true"></span>{{ copy.hero.eyebrow }}</p>
                <h1 class="lp-hero-title">{{ copy.hero.title }}</h1>
                <p class="lp-hero-lead">{{ copy.hero.lead }}</p>
                <div class="lp-hero-actions">
                    <a class="lp-btn lp-btn-primary" :href="copy.hero.actions[0].href">{{ copy.hero.actions[0].label }}</a>
                    <a class="lp-btn-text" :href="copy.hero.actions[1].href">{{ copy.hero.actions[1].label }}<span class="lp-arrow" aria-hidden="true">→</span></a>
                    <a class="lp-btn-text" :href="copy.hero.actions[2].href">{{ copy.hero.actions[2].label }}<span class="lp-arrow" aria-hidden="true">→</span></a>
                </div>
            </div>
            <div class="lp-container">
                <ul class="lp-specstrip">
                    <li v-for="(item, i) in copy.hero.trust" :key="item" class="lp-spec">
                        <span class="lp-spec-idx" aria-hidden="true">{{ pad(i + 1) }}</span>
                        <span class="lp-spec-text">{{ item }}</span>
                    </li>
                </ul>
            </div>
        </header>

        <!-- TERMINAL (animated) -->
        <section class="lp-section">
            <div class="lp-container">
                <div class="lp-terminal">
                    <div class="lp-terminal-head">
                        <span class="lp-terminal-path"><span class="lp-live" aria-hidden="true"></span>{{ copy.panel.title }}</span>
                        <span class="lp-terminal-meta">gflowd&nbsp;·&nbsp;127.0.0.1:5577</span>
                    </div>
                    <div class="lp-terminal-body">
                        <div
                            v-for="(row, i) in TERMINAL"
                            :key="i"
                            v-show="frame >= row.at"
                            :class="['lp-tl', `lp-tl-${row.kind}`]"
                        >
                            <template v-if="row.kind === 'cmd'">
                                <span class="lp-prompt">$</span><span class="lp-cmd-text">{{ row.text }}</span><span v-if="i === activeCmd" class="lp-cursor" aria-hidden="true"></span>
                            </template>
                            <template v-else-if="row.kind === 'thead'"><span v-for="c in row.cells" :key="c" class="lp-th">{{ c }}</span></template>
                            <template v-else-if="row.kind === 'trow'">
                                <span class="lp-td">{{ row.id }}</span><span class="lp-td">{{ row.name }}</span><span :class="['lp-td', 'lp-state', frame >= JOB_RUNS_AT ? 'is-run' : 'is-pend']">{{ frame >= JOB_RUNS_AT ? "RUNNING" : "PENDING" }}</span><span class="lp-td">{{ row.gpu }}</span><span class="lp-td">{{ row.node }}</span>
                            </template>
                            <template v-else>{{ row.text }}</template>
                        </div>
                    </div>
                </div>
            </div>
        </section>

        <!-- 01 · WHY -->
        <section class="lp-section">
            <div class="lp-container">
                <div class="lp-sec-head">
                    <p class="lp-sec-eyebrow"><span class="lp-sec-idx" aria-hidden="true">{{ pad(1) }}</span>{{ copy.problem.eyebrow }}</p>
                    <h2 class="lp-sec-title">{{ copy.problem.title }}</h2>
                    <p class="lp-sec-lead">{{ copy.problem.lead }}</p>
                </div>
                <div class="lp-grid lp-grid-2">
                    <div class="lp-cell">
                        <p class="lp-cell-label lp-cell-label-dim">{{ copy.problem.painTitle }}</p>
                        <ul class="lp-list lp-list-minus">
                            <li v-for="item in copy.problem.painItems" :key="item">{{ item }}</li>
                        </ul>
                    </div>
                    <div class="lp-cell">
                        <p class="lp-cell-label lp-cell-label-accent">{{ copy.problem.valueTitle }}</p>
                        <ul class="lp-list lp-list-plus">
                            <li v-for="item in copy.problem.valueItems" :key="item">{{ item }}</li>
                        </ul>
                    </div>
                </div>
            </div>
        </section>

        <!-- 02 · HOW IT WORKS (animated pipeline) -->
        <section class="lp-section">
            <div class="lp-container">
                <div class="lp-sec-head">
                    <p class="lp-sec-eyebrow"><span class="lp-sec-idx" aria-hidden="true">{{ pad(2) }}</span>{{ copy.workflow.eyebrow }}</p>
                    <h2 class="lp-sec-title">{{ copy.workflow.title }}</h2>
                </div>
                <div class="lp-flow">
                    <div v-for="(s, i) in copy.workflow.steps" :key="s.idx" class="lp-flow-stage" :style="({ '--i': i } as any)">
                        <span class="lp-flow-bar" aria-hidden="true"></span>
                        <span class="lp-flow-idx">{{ s.idx }}</span>
                        <code class="lp-flow-cmd">{{ s.cmd }}</code>
                        <span class="lp-flow-label">{{ s.label }}</span>
                    </div>
                </div>
            </div>
        </section>

        <!-- 03 · CAPABILITIES -->
        <section class="lp-section">
            <div class="lp-container">
                <div class="lp-sec-head">
                    <p class="lp-sec-eyebrow"><span class="lp-sec-idx" aria-hidden="true">{{ pad(3) }}</span>{{ copy.capabilities.eyebrow }}</p>
                    <h2 class="lp-sec-title">{{ copy.capabilities.title }}</h2>
                </div>
                <div class="lp-grid lp-grid-3">
                    <div class="lp-cell" v-for="(item, i) in copy.capabilities.items" :key="item.title">
                        <span class="lp-cell-idx" aria-hidden="true">{{ pad(i + 1) }}</span>
                        <h3 class="lp-cell-title">{{ item.title }}</h3>
                        <p class="lp-cell-body">{{ item.body }}</p>
                    </div>
                </div>
            </div>
        </section>

        <!-- 04 · WHERE IT FITS -->
        <section class="lp-section">
            <div class="lp-container">
                <div class="lp-sec-head">
                    <p class="lp-sec-eyebrow"><span class="lp-sec-idx" aria-hidden="true">{{ pad(4) }}</span>{{ copy.scenarios.eyebrow }}</p>
                    <h2 class="lp-sec-title">{{ copy.scenarios.title }}</h2>
                </div>
                <div class="lp-grid lp-grid-3">
                    <div class="lp-cell" v-for="item in copy.scenarios.items" :key="item.title">
                        <h3 class="lp-cell-title">{{ item.title }}</h3>
                        <p class="lp-cell-body">{{ item.body }}</p>
                    </div>
                </div>
            </div>
        </section>

        <!-- 05 · DOCUMENTATION -->
        <section class="lp-section">
            <div class="lp-container">
                <div class="lp-sec-head">
                    <p class="lp-sec-eyebrow"><span class="lp-sec-idx" aria-hidden="true">{{ pad(5) }}</span>{{ copy.pathways.eyebrow }}</p>
                    <h2 class="lp-sec-title">{{ copy.pathways.title }}</h2>
                </div>
                <div class="lp-table">
                    <a class="lp-table-row" v-for="item in copy.pathways.items" :key="item.href" :href="item.href">
                        <span class="lp-table-title">{{ item.title }}</span>
                        <span class="lp-table-body">{{ item.body }}</span>
                        <span class="lp-table-cta">{{ item.cta }}<span class="lp-arrow" aria-hidden="true">→</span></span>
                    </a>
                </div>
            </div>
        </section>

        <!-- AI / MCP BAND -->
        <section class="lp-band">
            <div class="lp-container lp-band-inner">
                <div class="lp-band-copy">
                    <p class="lp-label lp-label-inv"><span class="lp-label-tick lp-label-tick-inv" aria-hidden="true"></span>{{ copy.mcp.eyebrow }}</p>
                    <h2 class="lp-band-title">{{ copy.mcp.title }}</h2>
                    <p class="lp-band-lead">{{ copy.mcp.lead }}</p>
                </div>
                <div class="lp-band-action">
                    <pre class="lp-band-cmd"><code class="lp-band-cmd-text">{{ copy.mcp.command }}</code></pre>
                    <a class="lp-btn lp-btn-inv" :href="copy.mcp.href">{{ copy.mcp.cta }}<span class="lp-arrow" aria-hidden="true">→</span></a>
                </div>
            </div>
        </section>

        <!-- CLOSING -->
        <section class="lp-closing">
            <div class="lp-container">
                <h2 class="lp-closing-title">{{ copy.cta.title }}</h2>
                <p class="lp-closing-lead">{{ copy.cta.lead }}</p>
                <div class="lp-hero-actions">
                    <a class="lp-btn lp-btn-onblue" :href="copy.cta.actions[0].href">{{ copy.cta.actions[0].label }}</a>
                    <a class="lp-btn-text lp-btn-text-inv" :href="copy.cta.actions[1].href">{{ copy.cta.actions[1].label }}<span class="lp-arrow" aria-hidden="true">→</span></a>
                </div>
            </div>
        </section>
    </div>
</template>