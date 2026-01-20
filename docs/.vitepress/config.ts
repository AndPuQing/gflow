import { defineConfig } from "vitepress";
import { vitepressMermaidPreview } from "vitepress-mermaid-preview";

// https://vitepress.dev/reference/site-config
export default defineConfig({
    title: "gflow",
    description: "A lightweight, single-node job scheduler inspired by Slurm",
    base: "",
    srcDir: "src",

    locales: {
        root: {
            label: "English",
            lang: "en",
            themeConfig: {
                nav: [
                    { text: "Home", link: "/" },
                    {
                        text: "Getting Started",
                        link: "/getting-started/installation",
                    },
                    { text: "User Guide", link: "/user-guide/job-submission" },
                    { text: "Reference", link: "/reference/quick-reference" },
                ],

                sidebar: [
                    {
                        text: "Getting Started",
                        items: [
                            {
                                text: "Installation",
                                link: "/getting-started/installation",
                            },
                            {
                                text: "Quick Start",
                                link: "/getting-started/quick-start",
                            },
                        ],
                    },
                    {
                        text: "User Guide",
                        items: [
                            {
                                text: "Job Submission",
                                link: "/user-guide/job-submission",
                            },
                            {
                                text: "Job Dependencies",
                                link: "/user-guide/job-dependencies",
                            },
                            {
                                text: "Job Lifecycle",
                                link: "/user-guide/job-lifecycle",
                            },
                            {
                                text: "GPU Management",
                                link: "/user-guide/gpu-management",
                            },
                            {
                                text: "Time Limits",
                                link: "/user-guide/time-limits",
                            },
                            {
                                text: "Configuration",
                                link: "/user-guide/configuration",
                            },
                        ],
                    },
                    {
                        text: "Reference",
                        items: [
                            {
                                text: "Quick Reference",
                                link: "/reference/quick-reference",
                            },
                            {
                                text: "gbatch Reference",
                                link: "/reference/gbatch-reference",
                            },
                            {
                                text: "gqueue Reference",
                                link: "/reference/gqueue-reference",
                            },
                            {
                                text: "gcancel Reference",
                                link: "/reference/gcancel-reference",
                            },
                            {
                                text: "ginfo Reference",
                                link: "/reference/ginfo-reference",
                            },
                        ],
                    },
                ],

                editLink: {
                    pattern:
                        "https://github.com/AndPuQing/gflow/edit/main/docs/src/:path",
                    text: "Edit this page on GitHub",
                },
            },
        },
        "zh-CN": {
            label: "简体中文",
            lang: "zh-CN",
            themeConfig: {
                nav: [
                    { text: "首页", link: "/zh-CN/" },
                    {
                        text: "快速开始",
                        link: "/zh-CN/getting-started/installation",
                    },
                    {
                        text: "用户指南",
                        link: "/zh-CN/user-guide/job-submission",
                    },
                    {
                        text: "参考文档",
                        link: "/zh-CN/reference/quick-reference",
                    },
                ],

                sidebar: [
                    {
                        text: "快速开始",
                        items: [
                            {
                                text: "安装",
                                link: "/zh-CN/getting-started/installation",
                            },
                            {
                                text: "快速入门",
                                link: "/zh-CN/getting-started/quick-start",
                            },
                        ],
                    },
                    {
                        text: "用户指南",
                        items: [
                            {
                                text: "任务提交",
                                link: "/zh-CN/user-guide/job-submission",
                            },
                            {
                                text: "任务依赖",
                                link: "/zh-CN/user-guide/job-dependencies",
                            },
                            {
                                text: "任务生命周期",
                                link: "/zh-CN/user-guide/job-lifecycle",
                            },
                            {
                                text: "GPU 管理",
                                link: "/zh-CN/user-guide/gpu-management",
                            },
                            {
                                text: "时间限制",
                                link: "/zh-CN/user-guide/time-limits",
                            },
                            {
                                text: "配置",
                                link: "/zh-CN/user-guide/configuration",
                            },
                        ],
                    },
                    {
                        text: "参考文档",
                        items: [
                            {
                                text: "快速参考",
                                link: "/zh-CN/reference/quick-reference",
                            },
                            {
                                text: "gbatch 参考",
                                link: "/zh-CN/reference/gbatch-reference",
                            },
                            {
                                text: "gqueue 参考",
                                link: "/zh-CN/reference/gqueue-reference",
                            },
                            {
                                text: "gcancel 参考",
                                link: "/zh-CN/reference/gcancel-reference",
                            },
                            {
                                text: "ginfo 参考",
                                link: "/zh-CN/reference/ginfo-reference",
                            },
                        ],
                    },
                ],

                editLink: {
                    pattern:
                        "https://github.com/AndPuQing/gflow/edit/main/docs/src/:path",
                    text: "在 GitHub 上编辑此页",
                },

                docFooter: {
                    prev: "上一页",
                    next: "下一页",
                },

                outline: {
                    label: "页面导航",
                },

                lastUpdated: {
                    text: "最后更新于",
                    formatOptions: {
                        dateStyle: "short",
                        timeStyle: "medium",
                    },
                },

                langMenuLabel: "多语言",
                returnToTopLabel: "回到顶部",
                sidebarMenuLabel: "菜单",
                darkModeSwitchLabel: "主题",
                lightModeSwitchTitle: "切换到浅色模式",
                darkModeSwitchTitle: "切换到深色模式",
            },
        },
    },

    markdown: {
        config: (md) => {
            vitepressMermaidPreview(md, {
                showToolbar: false, // Global setting: whether to show toolbar by default
            });
        },
    },

    themeConfig: {
        // https://vitepress.dev/reference/default-theme-config
        logo: "/logo.svg",

        socialLinks: [
            { icon: "github", link: "https://github.com/AndPuQing/gflow" },
        ],

        search: {
            provider: "local",
        },

        footer: {
            message: "Released under the MIT License.",
            copyright: "Copyright © 2025-present PuQing",
        },
    },
});
