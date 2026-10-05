/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,jsx}'],
  theme: {
    extend: {
      colors: {
        espresso: { DEFAULT: '#241A15', 800: '#33261F', 700: '#4A382E', 600: '#6B5446' },
        bench: '#EDEEE9',
        line: '#D9DBD3',
        roast: { DEFAULT: '#9A5B2E', dark: '#7C4722', light: '#F3E6DA' },
        bean: { DEFAULT: '#6E7F4E', light: '#E7EBDD' },
        signal: { DEFAULT: '#2F6F8F', light: '#DEEAF0' },
        warn: { DEFAULT: '#B7791F', light: '#F6EAD3' },
        danger: { DEFAULT: '#B23A2E', light: '#F5DEDB' },
        ink: { DEFAULT: '#241A15', soft: '#5C5650', faint: '#8A857F' },
      },
      fontFamily: {
        sans: ['"IBM Plex Sans"', 'system-ui', 'Segoe UI', 'Roboto', 'sans-serif'],
        display: ['"IBM Plex Sans Condensed"', '"IBM Plex Sans"', 'system-ui', 'sans-serif'],
      },
      borderRadius: { panel: '14px' },
    },
  },
  plugins: [],
}
