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
            lead: "Queue, run, inspect, and control GPU or CPU jobs on a shared workstation with a small CLI and a local daemon. No cluster deployment required.",
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
                    label: "Operate queueing, runtime control, and inspection from familiar commands.",
                },
                {
                    value: "Recoverable",
                    label: "Attach to sessions, follow logs, and redo failed work without losing context.",
                },
                {
                    value: "Explicit policy",
                    label: "Set GPUs, VRAM limits, priorities, reservations, and dependencies with clear intent.",
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
            title: "When one machine becomes shared infrastructure",
            lead: "Shell history, ad hoc tmux sessions, and hand-managed GPU etiquette stop scaling the moment a workstation is shared.",
            painTitle: "Without queue discipline",
            painItems: [
                "Long jobs collide with interactive work and nobody has a system-wide view.",
                "Failures are hard to recover because process state, logs, and ownership are scattered.",
                "GPU requests become tribal knowledge instead of explicit scheduling rules.",
            ],
            valueTitle: "With gflow",
            valueItems: [
                "A local daemon keeps scheduling state, runtime control, and queue inspection consistent.",
                "Every job has a lifecycle you can list, filter, attach to, update, or redo.",
                "Resource requests, dependencies, arrays, and reservations are encoded at submission time.",
            ],
        },
        workflow: {
            eyebrow: "How it works",
            title: "A familiar flow in four commands",
            steps: [
                {
                    label: "01",
                    command: "gflowd up",
                    description: "Start the local scheduler inside a tmux-backed daemon session.",
                },
                {
                    label: "02",
                    command: "gbatch --gpus 1 python train.py",
                    description: "Submit a command or script with explicit resources, limits, and metadata.",
                },
                {
                    label: "03",
                    command: "gqueue",
                    description: "Inspect active, queued, or completed jobs in table, tree, or structured formats.",
                },
                {
                    label: "04",
                    command: "gjob log <job_id>",
                    description: "Follow logs, attach to the session, or intervene when a run needs attention.",
                },
            ],
        },
        capabilities: {
            eyebrow: "Capabilities",
            title: "Built for day-to-day workstation operations",
            items: [
                {
                    title: "Queueing and lifecycle",
                    body: "Submit, hold, release, cancel, update, and redo jobs with a state model you can inspect.",
                },
                {
                    title: "GPU-aware scheduling",
                    body: "Request GPUs directly, enable shared mode, and place VRAM limits on jobs that can coexist.",
                },
                {
                    title: "Workflow composition",
                    body: "Combine dependencies, arrays, and parameter sweeps for multi-stage experiments.",
                },
                {
                    title: "Operational visibility",
                    body: "Read queue state through tables, trees, JSON, CSV, or YAML without bespoke wrappers.",
                },
                {
                    title: "Recoverable execution",
                    body: "Run every job in its own tmux session so logs and interactive recovery stay close to the process.",
                },
                {
                    title: "Automation and AI integration",
                    body: "Expose scheduler actions through the local MCP server for Codex, Claude Code, and similar tools.",
                },
            ],
        },
        scenarios: {
            eyebrow: "Where it fits",
            title: "Common workstation scenarios",
            items: [
                {
                    title: "Shared lab GPU server",
                    body: "Coordinate multiple researchers on one box without turning daily scheduling into Slack negotiations.",
                },
                {
                    title: "Solo research machine",
                    body: "Keep long-running experiments structured, restartable, and visible even when nobody else uses the host.",
                },
                {
                    title: "Local evaluation pipelines",
                    body: "Chain prep, train, benchmark, and reporting jobs with dependencies instead of hand-written shell choreography.",
                },
            ],
        },
        pathways: {
            eyebrow: "Documentation paths",
            title: "Start where you are",
            items: [
                {
                    title: "Install and launch",
                    body: "Set up the binary, create config defaults, and bring the daemon online.",
                    href: "/getting-started/installation",
                    cta: "Open Installation",
                },
                {
                    title: "Learn the first workflow",
                    body: "Follow the shortest path from daemon startup to job submission, queue inspection, and logs.",
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
                    body: "Run `gflow` as a local MCP server for Codex, Claude Code, and other agent tooling.",
                    href: "/ai-integration/mcp-and-skills",
                    cta: "Open AI Integration",
                },
            ],
        },
        mcp: {
            eyebrow: "AI integration",
            title: "Make scheduler operations available as tools",
            lead: "Run gflow as a local stdio MCP server so agent CLIs can inspect queues, summarize logs, and drive scheduler workflows without reconstructing shell commands each time.",
            command: "gflow mcp serve",
            href: "/ai-integration/mcp-and-skills",
            cta: "Read Agents, MCP, and Skills",
        },
        cta: {
            title: "Start scheduling the machine you already have",
            lead: "Use the docs as an operator handbook, not just a command dump.",
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
            lead: "在共享工作站上，用轻量 CLI 和本地 daemon 完成 GPU 或 CPU 任务的提交、排队、查看与控制，不必部署整套集群系统。",
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
                    label: "通过熟悉的命令行完成排队、运行时控制和队列查看。",
                },
                {
                    value: "可恢复",
                    label: "随时 attach 会话、追踪日志，并在失败后重做任务而不丢失上下文。",
                },
                {
                    value: "策略明确",
                    label: "用清晰的资源声明表达 GPU、显存、优先级、预约和依赖关系。",
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
            title: "当一台机器变成共享基础设施",
            lead: "一旦工作站开始被多人或多个实验共享，shell 历史、临时 tmux 会话和口头约定就很快失效。",
            painTitle: "没有队列纪律时",
            painItems: [
                "长任务会和交互式工作互相打架，而且没人能看到全局状态。",
                "失败后很难恢复，因为进程、日志和责任边界都散落在不同地方。",
                "GPU 使用规则只能靠经验相传，而不是明确的调度策略。",
            ],
            valueTitle: "使用 gflow 后",
            valueItems: [
                "本地 daemon 统一保存调度状态、运行时控制和队列视图。",
                "每个任务都有可查看、可筛选、可 attach、可更新、可重做的生命周期。",
                "资源请求、依赖、数组任务和预约都在提交时明确表达出来。",
            ],
        },
        workflow: {
            eyebrow: "工作流",
            title: "四条命令进入可控状态",
            steps: [
                {
                    label: "01",
                    command: "gflowd up",
                    description: "在 tmux 支撑的 daemon 会话中启动本地调度器。",
                },
                {
                    label: "02",
                    command: "gbatch --gpus 1 python train.py",
                    description: "用明确的资源、限制和元信息提交命令或脚本。",
                },
                {
                    label: "03",
                    command: "gqueue",
                    description: "以表格、树状或结构化格式查看运行中、排队中或已完成任务。",
                },
                {
                    label: "04",
                    command: "gjob log <job_id>",
                    description: "追踪日志、attach 会话，并在任务需要介入时快速处理。",
                },
            ],
        },
        capabilities: {
            eyebrow: "能力概览",
            title: "面向日常工作站运维的设计",
            items: [
                {
                    title: "队列与生命周期",
                    body: "支持提交、挂起、恢复、取消、更新与重做，并拥有可检查的状态模型。",
                },
                {
                    title: "GPU 感知调度",
                    body: "直接声明 GPU 数量，开启共享模式，并为可并行运行的任务设置显存上限。",
                },
                {
                    title: "工作流编排",
                    body: "通过依赖、数组任务和参数扫描组合出多阶段实验流程。",
                },
                {
                    title: "可观测性",
                    body: "无需额外封装即可通过表格、树状、JSON、CSV 或 YAML 查看队列状态。",
                },
                {
                    title: "可恢复执行",
                    body: "每个任务都运行在独立 tmux 会话中，日志、attach 和恢复都更直接。",
                },
                {
                    title: "自动化与 AI 集成",
                    body: "通过本地 MCP server 暴露调度操作，方便 Codex、Claude Code 等 Agent 调用。",
                },
            ],
        },
        scenarios: {
            eyebrow: "适用场景",
            title: "常见的工作站使用方式",
            items: [
                {
                    title: "共享实验室 GPU 服务器",
                    body: "多人共用一台机器时，用明确的调度规则替代口头协调和即时沟通。",
                },
                {
                    title: "个人研究主机",
                    body: "即使机器只给自己使用，也能让长时间实验保持结构化、可恢复、可追踪。",
                },
                {
                    title: "本地评测与自动化流水线",
                    body: "用依赖关系串联预处理、训练、评测和汇总，而不是堆叠脆弱的 shell 脚本。",
                },
            ],
        },
        pathways: {
            eyebrow: "文档入口",
            title: "按你的阶段开始",
            items: [
                {
                    title: "安装并启动",
                    body: "完成二进制安装、生成默认配置，并启动 daemon。",
                    href: "/zh-CN/getting-started/installation",
                    cta: "查看安装指南",
                },
                {
                    title: "学习第一个流程",
                    body: "从启动调度器到提交任务、查看队列、读取日志，走完最短路径。",
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
                    body: "把 `gflow` 作为本地 MCP server 接给 Codex、Claude Code 等工具。",
                    href: "/zh-CN/ai-integration/mcp-and-skills",
                    cta: "查看 AI 集成",
                },
            ],
        },
        mcp: {
            eyebrow: "AI 集成",
            title: "把调度操作暴露成 Agent 可调用的工具",
            lead: "将 gflow 作为本地 stdio MCP server 运行后，Agent CLI 可以直接查看队列、总结日志并驱动调度流程，而不必反复拼接 shell 命令。",
            command: "gflow mcp serve",
            href: "/zh-CN/ai-integration/mcp-and-skills",
            cta: "阅读 Agent、MCP 与 Skill",
        },
        cta: {
            title: "从你已经拥有的那台机器开始调度",
            lead: "把这套文档当作运维手册，而不是单纯的命令堆叠。",
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
