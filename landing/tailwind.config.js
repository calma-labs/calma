/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ['./*.html'],
  theme: {
    extend: {
      colors: {
        base:   '#FFFFFF',
        ink:    '#F6F8FB',
        surface:'#EDF1F8',
        rule:   '#D6DCEA',
        muted:  '#6B7299',
        soft:   '#3F4866',
        fg:     '#0A0E17',
        accent: '#0A0E17',
        accent2:'#3F4866',
      },
      fontFamily: {
        sans: ['Inter', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'ui-monospace', 'monospace'],
      },
      letterSpacing: {
        tightest: '-0.04em',
        tighter:  '-0.025em',
      },
      maxWidth: {
        page: '1200px',
      },
    }
  }
}
