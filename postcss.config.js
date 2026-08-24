export default ({ file }) => {
  // Don't run postcss on Svelte virtual style modules - they are handled by svelte plugin
  if (file && file.includes("?svelte")) {
    return { plugins: {} };
  }
  return {
    plugins: {
      tailwindcss: {},
      autoprefixer: {},
    },
  };
};
