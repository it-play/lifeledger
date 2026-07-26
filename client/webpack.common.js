const path = require('node:path');
const CopyPlugin = require('copy-webpack-plugin');

module.exports = {
  entry: {
    app: './src/main.ts',
  },
  output: {
    path: path.resolve(__dirname, 'dist'),
    clean: true,
    filename: './js/app.js',
  },
  resolve: {
    extensions: ['.ts', '.js'],
    // ESM 규칙대로 소스에서는 './x.js' 로 쓰고, 실제 파일은 x.ts 를 찾게 한다
    extensionAlias: {
      '.js': ['.ts', '.js'],
    },
  },
  module: {
    rules: [
      {
        test: /\.ts$/,
        exclude: /node_modules/,
        loader: 'esbuild-loader',
        options: {
          target: 'es2022',
        },
      },
    ],
  },
  plugins: [
    new CopyPlugin({
      patterns: [{ from: 'node_modules/uplot/dist/uPlot.min.css', to: 'css/uPlot.min.css' }],
    }),
  ],
};
