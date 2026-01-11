// https://vitepress.dev/reference/site-config
export default ({
  title: "gflow",
  description: "A lightweight, single-node job scheduler inspired by Slurm",
  base: '/gflow/',
  srcDir: 'src',

  themeConfig: {
    // https://vitepress.dev/reference/default-theme-config
    logo: '/logo.svg',

    nav: [
      { text: 'Home', link: '/' },
      { text: 'Getting Started', link: '/getting-started/installation' },
      { text: 'User Guide', link: '/user-guide/job-submission' },
      { text: 'Reference', link: '/reference/quick-reference' }
    ],

    sidebar: [
      {
        text: 'Getting Started',
        items: [
          { text: 'Installation', link: '/getting-started/installation' },
          { text: 'Quick Start', link: '/getting-started/quick-start' }
        ]
      },
      {
        text: 'User Guide',
        items: [
          { text: 'Job Submission', link: '/user-guide/job-submission' },
          { text: 'Job Dependencies', link: '/user-guide/job-dependencies' },
          { text: 'GPU Management', link: '/user-guide/gpu-management' },
          { text: 'Time Limits', link: '/user-guide/time-limits' },
          { text: 'Configuration', link: '/user-guide/configuration' }
        ]
      },
      {
        text: 'Reference',
        items: [
          { text: 'Quick Reference', link: '/reference/quick-reference' },
          { text: 'gbatch Reference', link: '/reference/gbatch-reference' },
          { text: 'gqueue Reference', link: '/reference/gqueue-reference' },
          { text: 'gcancel Reference', link: '/reference/gcancel-reference' },
          { text: 'ginfo Reference', link: '/reference/ginfo-reference' }
        ]
      }
    ],

    socialLinks: [
        { icon: 'github', link: 'https://github.com/AndPuQing/gflow' }
    ],

    search: {
      provider: 'local'
    },

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright © 2024-present PuQing'
    },

    editLink: {
        pattern: 'https://github.com/AndPuQing/gflow/edit/main/docs/src/:path',
      text: 'Edit this page on GitHub'
    }
  }
})
