const path = require("node:path");

const projectSrc = path.join(__dirname, "src") + path.sep;

module.exports = function (api) {
  api.cache(true);
  return {
    presets: [
      [
        "babel-preset-expo",
        {
          "react-compiler": {
            sources: (filename) => filename.startsWith(projectSrc),
          },
        },
      ],
    ],
    plugins: ["@lingui/babel-plugin-lingui-macro"],
  };
};
