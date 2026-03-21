<script setup lang="ts">
import { computed } from "vue";
import { createHighlighterCoreSync } from "@shikijs/core";
import { createJavaScriptRegexEngine } from "@shikijs/engine-javascript";
import bash from "@shikijs/langs/bash";
import githubDark from "@shikijs/themes/github-dark";
import githubLight from "@shikijs/themes/github-light";

const props = defineProps<{
    locale?: "en" | "zh-CN";
}>();

const highlighter = createHighlighterCoreSync({
    engine: createJavaScriptRegexEngine(),
    themes: [githubLight, githubDark],
    langs: [bash],
});

const highlightThemes = {
    light: "github-light",
    dark: "github-dark",
} as const;

function escapeHtml(value: string) {
    return value
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")
        .replaceAll("'", "&#39;");
}

function extractCodeHtml(html: string) {
    const match = html.match(/<code[^>]*>([\s\S]*?)<\/code>/);
    return match?.[1] ?? html;
}

function highlightShell(code: string) {
    let placeholderIndex = 0;
    const placeholders = new Map<string, string>();
    const normalizedCode = code.replace(/<[^>\n]+>/g, (placeholder) => {
        const token = `GFLOWPLACEHOLDER${placeholderIndex++}`;
        placeholders.set(token, placeholder);
        return token;
    });

    let html = extractCodeHtml(
        highlighter.codeToHtml(normalizedCode, {
            lang: "bash",
            themes: highlightThemes,
        }),
    );

    for (const [token, placeholder] of placeholders) {
        html = html.replace(token, `<span class="lp-code-placeholder">${escapeHtml(placeholder)}</span>`);
    }

    return html;
}

function classifyTerminalLine(line: string) {
    if (line.startsWith("$ ")) {
        return {
            kind: "command",
            html: `<span class="lp-terminal-prompt">$</span> ${highlightShell(line.slice(2))}`,
        };
    }

    if (line.startsWith("JOBID")) {
        return {
            kind: "header",
            html: escapeHtml(line),
        };
    }

    if (/^\d+\s+/.test(line)) {
        return {
            kind: "table",
            html: escapeHtml(line),
        };
    }

    if (line.includes("=")) {
        return {
            kind: "metrics",
            html: escapeHtml(line),
        };
    }

    if (line.startsWith("daemon ready") || line.startsWith("submitted batch job")) {
        return {
            kind: "success",
            html: escapeHtml(line),
        };
    }

    return {
        kind: "output",
        html: escapeHtml(line),
    };
}

const copies = {
    en: {
        hero: {
            eyebrow: "Single-node scheduling for shared machines",
            title: "Give one Linux machine a real job scheduler",
            lead: "Queue, run, and inspect GPU or CPU jobs on a shared workstation with a small CLI and a local daemon.",
            actions: [
                { label: "Quick Start", href: "/getting-started/quick-start", kind: "brand" },
                { label: "Installation", href: "/getting-started/installation", kind: "alt" },
                { label: "Command Reference", href: "/reference/quick-reference", kind: "ghost" },
            ],
            trust: [
                "Single-node by design",
                "GPU-aware scheduling",
                "tmux-backed execution",
                "MCP-ready for agents",
            ],
            stats: [
                {
                    value: "CLI-first",
                    label: "Queueing, control, and inspection from familiar commands.",
                },
                {
                    value: "Recoverable",
                    label: "Attach, follow logs, and redo failed work.",
                },
                {
                    value: "Explicit policy",
                    label: "Declare GPUs, VRAM limits, priorities, reservations, and dependencies.",
                },
            ],
        },
        panel: {
            title: "Workstation session",
            lines: [
                "$ gflowd up",
                "daemon ready on 127.0.0.1:5577",
                "$ gbatch --gpus 1 --project vision python train.py",
                "submitted batch job 184",
                "$ gqueue",
                "JOBID  NAME    ST  TIME      GPU  NODELIST(REASON)",
                "184    train   R   00:12:41  1    0",
                "185    eval    PD  -         0    (WaitingForDependency)",
                "$ gjob log 184",
                "step=420 loss=0.184 throughput=178 img/s",
            ],
            queueCardTitle: "Queue snapshot",
            queueCardValues: [
                { label: "Running", value: "04" },
                { label: "Queued", value: "09" },
                { label: "Held", value: "01" },
            ],
            gpuCardTitle: "GPU policy",
            gpuCardValues: [
                { label: "Visible", value: "0,1,2,3" },
                { label: "Shared", value: "VRAM-aware" },
            ],
        },
        problem: {
            eyebrow: "Why it exists",
            title: "When one machine is no longer just yours",
            lead: "Ad hoc tmux sessions and manual GPU etiquette break down once a workstation is shared.",
            painTitle: "Without queue discipline",
            painItems: [
                "Long jobs collide with interactive work.",
                "Failures are hard to recover because state and logs are scattered.",
                "GPU requests become tribal knowledge instead of policy.",
            ],
            valueTitle: "With gflow",
            valueItems: [
                "A local daemon keeps state and queue control consistent.",
                "Every job has a lifecycle you can inspect and recover.",
                "Resources and dependencies are declared at submission time.",
            ],
        },
        workflow: {
            eyebrow: "How it works",
            title: "A familiar flow in four commands",
            steps: [
                {
                    label: "01",
                    command: "gflowd up",
                    description: "Start the local scheduler.",
                },
                {
                    label: "02",
                    command: "gbatch --gpus 1 python train.py",
                    description: "Submit a command or script with explicit resources.",
                },
                {
                    label: "03",
                    command: "gqueue",
                    description: "Inspect running, queued, or completed jobs.",
                },
                {
                    label: "04",
                    command: "gjob log <job_id>",
                    description: "Follow logs or attach when a run needs attention.",
                },
            ],
        },
        capabilities: {
            eyebrow: "Capabilities",
            title: "Built for daily workstation use",
            items: [
                {
                    title: "Queueing and lifecycle",
                    body: "Submit, hold, release, cancel, update, and redo jobs with an inspectable state model.",
                },
                {
                    title: "GPU-aware scheduling",
                    body: "Request GPUs directly, enable shared mode, and set VRAM limits.",
                },
                {
                    title: "Workflow composition",
                    body: "Use dependencies, arrays, and parameter sweeps for multi-stage runs.",
                },
                {
                    title: "Operational visibility",
                    body: "Read queue state through tables, trees, JSON, CSV, or YAML.",
                },
                {
                    title: "Recoverable execution",
                    body: "Run each job in its own tmux session for direct logs and recovery.",
                },
                {
                    title: "Automation and AI integration",
                    body: "Expose scheduler actions through a local MCP server.",
                },
            ],
        },
        scenarios: {
            eyebrow: "Where it fits",
            title: "Common scenarios",
            items: [
                {
                    title: "Shared lab GPU server",
                    body: "Coordinate multiple researchers on one box with explicit scheduling rules.",
                },
                {
                    title: "Solo research machine",
                    body: "Keep long-running experiments structured and restartable.",
                },
                {
                    title: "Local evaluation pipelines",
                    body: "Chain prep, train, benchmark, and reporting jobs with dependencies.",
                },
            ],
        },
        pathways: {
            eyebrow: "Documentation paths",
            title: "Start where you are",
            items: [
                {
                    title: "Install and launch",
                    body: "Install the binary, create config defaults, and start the daemon.",
                    href: "/getting-started/installation",
                    cta: "Open Installation",
                },
                {
                    title: "Learn the first workflow",
                    body: "Start the daemon, submit a job, inspect the queue, and read logs.",
                    href: "/getting-started/quick-start",
                    cta: "Open Quick Start",
                },
                {
                    title: "Jump to command reference",
                    body: "Use the cheat sheet for `gflowd`, `gbatch`, `gqueue`, `gjob`, `gctl`, and more.",
                    href: "/reference/quick-reference",
                    cta: "Open Reference",
                },
                {
                    title: "Connect your agents",
                    body: "Run `gflow` as a local MCP server for agent tooling.",
                    href: "/ai-integration/mcp-and-skills",
                    cta: "Open AI Integration",
                },
            ],
        },
        mcp: {
            eyebrow: "AI integration",
            title: "Make scheduler operations available as tools",
            lead: "Run gflow as a local stdio MCP server so agent CLIs can inspect queues and drive scheduler workflows.",
            command: "gflow mcp serve",
            href: "/ai-integration/mcp-and-skills",
            cta: "Read Agents, MCP, and Skills",
        },
        cta: {
            title: "Start scheduling the machine you already have",
            lead: "Use the docs as an operator handbook.",
            actions: [
                { label: "Install gflow", href: "/getting-started/installation", kind: "brand" },
                { label: "Read Quick Start", href: "/getting-started/quick-start", kind: "alt" },
            ],
        },
    },
    "zh-CN": {
        hero: {
            eyebrow: "面向共享机器的单节点调度",
            title: "让一台 Linux 机器拥有真正的任务调度器",
            lead: "在共享工作站上，用轻量 CLI 和本地 daemon 完成 GPU 或 CPU 任务的提交、排队与查看。",
            actions: [
                { label: "快速开始", href: "/zh-CN/getting-started/quick-start", kind: "brand" },
                { label: "安装指南", href: "/zh-CN/getting-started/installation", kind: "alt" },
                { label: "命令速查", href: "/zh-CN/reference/quick-reference", kind: "ghost" },
            ],
            trust: [
                "单节点设计",
                "GPU 感知调度",
                "基于 tmux 的执行",
                "可供 Agent 使用的 MCP",
            ],
            stats: [
                {
                    value: "CLI 优先",
                    label: "用熟悉的命令完成排队、控制和查看。",
                },
                {
                    value: "可恢复",
                    label: "随时 attach、追踪日志、重做任务。",
                },
                {
                    value: "策略明确",
                    label: "明确声明 GPU、显存、优先级、预约和依赖。",
                },
            ],
        },
        panel: {
            title: "工作站会话",
            lines: [
                "$ gflowd up",
                "daemon ready on 127.0.0.1:5577",
                "$ gbatch --gpus 1 --project vision python train.py",
                "submitted batch job 184",
                "$ gqueue",
                "JOBID  NAME    ST  TIME      GPU  NODELIST(REASON)",
                "184    train   R   00:12:41  1    0",
                "185    eval    PD  -         0    (WaitingForDependency)",
                "$ gjob log 184",
                "step=420 loss=0.184 throughput=178 img/s",
            ],
            queueCardTitle: "队列快照",
            queueCardValues: [
                { label: "运行中", value: "04" },
                { label: "排队中", value: "09" },
                { label: "挂起", value: "01" },
            ],
            gpuCardTitle: "GPU 策略",
            gpuCardValues: [
                { label: "可见设备", value: "0,1,2,3" },
                { label: "共享方式", value: "按显存调度" },
            ],
        },
        problem: {
            eyebrow: "为什么需要它",
            title: "当一台机器不再只属于你",
            lead: "一旦工作站开始共享，临时 tmux 会话和口头约定很快就会失效。",
            painTitle: "没有队列纪律时",
            painItems: [
                "长任务会和交互式工作互相打架。",
                "失败后难以恢复，因为状态和日志散落在不同地方。",
                "GPU 使用规则只能靠经验相传。",
            ],
            valueTitle: "使用 gflow 后",
            valueItems: [
                "本地 daemon 统一保存状态和队列控制。",
                "每个任务都有可查看、可恢复的生命周期。",
                "资源和依赖在提交时明确声明。",
            ],
        },
        workflow: {
            eyebrow: "工作流",
            title: "四条命令进入可控状态",
            steps: [
                {
                    label: "01",
                    command: "gflowd up",
                    description: "启动本地调度器。",
                },
                {
                    label: "02",
                    command: "gbatch --gpus 1 python train.py",
                    description: "用明确的资源声明提交命令或脚本。",
                },
                {
                    label: "03",
                    command: "gqueue",
                    description: "查看运行中、排队中或已完成任务。",
                },
                {
                    label: "04",
                    command: "gjob log <job_id>",
                    description: "追踪日志，或在需要时 attach 会话。",
                },
            ],
        },
        capabilities: {
            eyebrow: "能力概览",
            title: "为日常工作站使用而设计",
            items: [
                {
                    title: "队列与生命周期",
                    body: "支持提交、挂起、恢复、取消、更新与重做，并提供可检查的状态模型。",
                },
                {
                    title: "GPU 感知调度",
                    body: "直接声明 GPU 数量，开启共享模式，并设置显存上限。",
                },
                {
                    title: "工作流编排",
                    body: "通过依赖、数组任务和参数扫描组织多阶段任务。",
                },
                {
                    title: "可观测性",
                    body: "通过表格、树状、JSON、CSV 或 YAML 查看队列状态。",
                },
                {
                    title: "可恢复执行",
                    body: "每个任务都运行在独立 tmux 会话中，便于日志查看和恢复。",
                },
                {
                    title: "自动化与 AI 集成",
                    body: "通过本地 MCP server 暴露调度操作，供 Agent 调用。",
                },
            ],
        },
        scenarios: {
            eyebrow: "适用场景",
            title: "常见场景",
            items: [
                {
                    title: "共享实验室 GPU 服务器",
                    body: "多人共用一台机器时，用明确规则替代口头协调。",
                },
                {
                    title: "个人研究主机",
                    body: "让长时间实验保持结构化、可恢复。",
                },
                {
                    title: "本地评测与自动化流水线",
                    body: "用依赖关系串联预处理、训练、评测和汇总。",
                },
            ],
        },
        pathways: {
            eyebrow: "文档入口",
            title: "按你的阶段开始",
            items: [
                {
                    title: "安装并启动",
                    body: "安装二进制、生成默认配置，并启动 daemon。",
                    href: "/zh-CN/getting-started/installation",
                    cta: "查看安装指南",
                },
                {
                    title: "学习第一个流程",
                    body: "从启动调度器到提交任务、查看队列、读取日志。",
                    href: "/zh-CN/getting-started/quick-start",
                    cta: "查看快速开始",
                },
                {
                    title: "直接查命令",
                    body: "快速浏览 `gflowd`、`gbatch`、`gqueue`、`gjob`、`gctl` 等命令。",
                    href: "/zh-CN/reference/quick-reference",
                    cta: "查看命令速查",
                },
                {
                    title: "连接你的 Agent",
                    body: "把 `gflow` 作为本地 MCP server 接给 Agent 工具。",
                    href: "/zh-CN/ai-integration/mcp-and-skills",
                    cta: "查看 AI 集成",
                },
            ],
        },
        mcp: {
            eyebrow: "AI 集成",
            title: "把调度操作暴露成 Agent 可调用的工具",
            lead: "将 gflow 作为本地 stdio MCP server 运行后，Agent CLI 可以直接查看队列并驱动调度流程。",
            command: "gflow mcp serve",
            href: "/zh-CN/ai-integration/mcp-and-skills",
            cta: "阅读 Agent、MCP 与 Skill",
        },
        cta: {
            title: "从你已经拥有的那台机器开始调度",
            lead: "把这套文档当作运维手册。",
            actions: [
                { label: "安装 gflow", href: "/zh-CN/getting-started/installation", kind: "brand" },
                { label: "阅读快速开始", href: "/zh-CN/getting-started/quick-start", kind: "alt" },
            ],
        },
    },
} as const;

const currentLocale = computed(() => (props.locale === "zh-CN" ? "zh-CN" : "en"));
const copy = computed(() => copies[currentLocale.value]);
const terminalLines = computed(() => copy.value.panel.lines.map(classifyTerminalLine));
const workflowSteps = computed(() =>
    copy.value.workflow.steps.map((step) => ({
        ...step,
        commandHtml: highlightShell(step.command),
    })),
);
const mcpCommandHtml = computed(() => highlightShell(copy.value.mcp.command));
</script>

<template>
    <div class="landing-page">
        <section class="lp-hero">
            <div class="lp-hero-copy">
                <div class="lp-brand">
                    <img alt="gflow" class="lp-brand-logo" src="/logo.svg" />
                    <span>gflow</span>
                </div>
                <p class="lp-eyebrow">{{ copy.hero.eyebrow }}</p>
                <h1 class="lp-title">{{ copy.hero.title }}</h1>
                <p class="lp-lead">{{ copy.hero.lead }}</p>
                <div class="lp-actions">
                    <a
                        v-for="action in copy.hero.actions"
                        :key="action.href"
                        :class="['lp-button', `lp-button-${action.kind}`]"
                        :href="action.href"
                    >
                        {{ action.label }}
                    </a>
                </div>
                <ul class="lp-trust">
                    <li v-for="item in copy.hero.trust" :key="item">{{ item }}</li>
                </ul>
                <div class="lp-stats">
                    <article v-for="item in copy.hero.stats" :key="item.value" class="lp-stat-card">
                        <p class="lp-stat-value">{{ item.value }}</p>
                        <p class="lp-stat-label">{{ item.label }}</p>
                    </article>
                </div>
            </div>

            <div class="lp-hero-visual">
                <div class="lp-terminal">
                    <div class="lp-terminal-bar">
                        <span />
                        <span />
                        <span />
                        <strong>{{ copy.panel.title }}</strong>
                    </div>
                    <div class="lp-terminal-body">
                        <div
                            v-for="(line, index) in terminalLines"
                            :key="`${currentLocale}-${index}`"
                            :class="['lp-terminal-line', `lp-terminal-line-${line.kind}`]"
                            v-html="line.html"
                        />
                    </div>
                </div>
                <article class="lp-floating-card lp-floating-card-queue">
                    <p class="lp-floating-title">{{ copy.panel.queueCardTitle }}</p>
                    <div class="lp-floating-grid">
                        <div v-for="item in copy.panel.queueCardValues" :key="item.label">
                            <span>{{ item.label }}</span>
                            <strong>{{ item.value }}</strong>
                        </div>
                    </div>
                </article>
                <article class="lp-floating-card lp-floating-card-gpu">
                    <p class="lp-floating-title">{{ copy.panel.gpuCardTitle }}</p>
                    <div class="lp-floating-stack">
                        <div v-for="item in copy.panel.gpuCardValues" :key="item.label">
                            <span>{{ item.label }}</span>
                            <strong>{{ item.value }}</strong>
                        </div>
                    </div>
                </article>
            </div>
        </section>

        <section class="lp-section">
            <div class="lp-section-heading">
                <p class="lp-eyebrow">{{ copy.problem.eyebrow }}</p>
                <h2>{{ copy.problem.title }}</h2>
                <p>{{ copy.problem.lead }}</p>
            </div>
            <div class="lp-compare">
                <article class="lp-compare-card">
                    <p class="lp-compare-label">{{ copy.problem.painTitle }}</p>
                    <ul>
                        <li v-for="item in copy.problem.painItems" :key="item">{{ item }}</li>
                    </ul>
                </article>
                <article class="lp-compare-card lp-compare-card-accent">
                    <p class="lp-compare-label">{{ copy.problem.valueTitle }}</p>
                    <ul>
                        <li v-for="item in copy.problem.valueItems" :key="item">{{ item }}</li>
                    </ul>
                </article>
            </div>
        </section>

        <section class="lp-section">
            <div class="lp-section-heading">
                <p class="lp-eyebrow">{{ copy.workflow.eyebrow }}</p>
                <h2>{{ copy.workflow.title }}</h2>
            </div>
            <div class="lp-step-grid">
                <article v-for="step in workflowSteps" :key="step.label" class="lp-step-card">
                    <span class="lp-step-label">{{ step.label }}</span>
                    <code class="lp-code-inline" v-html="step.commandHtml"></code>
                    <p>{{ step.description }}</p>
                </article>
            </div>
        </section>

        <section class="lp-section">
            <div class="lp-section-heading">
                <p class="lp-eyebrow">{{ copy.capabilities.eyebrow }}</p>
                <h2>{{ copy.capabilities.title }}</h2>
            </div>
            <div class="lp-card-grid">
                <article v-for="item in copy.capabilities.items" :key="item.title" class="lp-card">
                    <h3>{{ item.title }}</h3>
                    <p>{{ item.body }}</p>
                </article>
            </div>
        </section>

        <section class="lp-section">
            <div class="lp-section-heading">
                <p class="lp-eyebrow">{{ copy.scenarios.eyebrow }}</p>
                <h2>{{ copy.scenarios.title }}</h2>
            </div>
            <div class="lp-card-grid lp-card-grid-scenarios">
                <article v-for="item in copy.scenarios.items" :key="item.title" class="lp-card lp-card-scenario">
                    <h3>{{ item.title }}</h3>
                    <p>{{ item.body }}</p>
                </article>
            </div>
        </section>

        <section class="lp-section">
            <div class="lp-section-heading">
                <p class="lp-eyebrow">{{ copy.pathways.eyebrow }}</p>
                <h2>{{ copy.pathways.title }}</h2>
            </div>
            <div class="lp-card-grid">
                <article v-for="item in copy.pathways.items" :key="item.href" class="lp-card lp-card-link">
                    <h3>{{ item.title }}</h3>
                    <p>{{ item.body }}</p>
                    <a :href="item.href">{{ item.cta }}</a>
                </article>
            </div>
        </section>

        <section class="lp-section lp-mcp-callout">
            <div class="lp-section-heading">
                <p class="lp-eyebrow">{{ copy.mcp.eyebrow }}</p>
                <h2>{{ copy.mcp.title }}</h2>
                <p>{{ copy.mcp.lead }}</p>
            </div>
            <div class="lp-mcp-row">
                <pre><code class="lp-code-inline" v-html="mcpCommandHtml"></code></pre>
                <a class="lp-button lp-button-brand" :href="copy.mcp.href">{{ copy.mcp.cta }}</a>
            </div>
        </section>

        <section class="lp-closing">
            <h2>{{ copy.cta.title }}</h2>
            <p>{{ copy.cta.lead }}</p>
            <div class="lp-actions">
                <a
                    v-for="action in copy.cta.actions"
                    :key="action.href"
                    :class="['lp-button', `lp-button-${action.kind}`]"
                    :href="action.href"
                >
                    {{ action.label }}
                </a>
            </div>
        </section>
    </div>
</template>
