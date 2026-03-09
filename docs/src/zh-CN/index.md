---
layout: home

hero:
  name: "gflow"
  text: "单节点任务调度器"
  tagline: "在一台 Linux 机器上完成任务提交、排队、查看与控制，无需部署完整集群"
  actions:
    - theme: brand
      text: 快速开始
      link: /zh-CN/getting-started/quick-start
    - theme: alt
      text: 安装指南
      link: /zh-CN/getting-started/installation
    - theme: alt
      text: 命令速查
      link: /zh-CN/reference/quick-reference
  image:
    src: /logo.svg
    alt: gflow

features:
  - icon: 🚀
    title: 轻量易部署
    details: 为单台机器补上排队与调度能力，不需要引入完整集群系统。
  - icon: 📋
    title: 灵活的任务提交
    details: 支持命令或脚本提交，并可设置 GPU 请求、优先级、时间限制、Conda 环境和数组任务。
  - icon: 🔗
    title: 依赖与数组任务
    details: 轻松编排前后置任务、批量展开实验，适合小型研究流水线与日常自动化。
  - icon: 📊
    title: 队列可观测性
    details: 支持查看活跃或已完成任务，并按用户、项目或状态筛选，支持表格、树状和结构化输出。
  - icon: 🖥️
    title: 基于 tmux 的执行
    details: 每个任务独占一个 tmux 会话，方便 attach、追踪日志和恢复长时间运行的任务。
  - icon: 🎛️
    title: GPU 感知控制
    details: 可限制可分配 GPU、基于显存做共享调度，并在运行期调整控制策略。
---
