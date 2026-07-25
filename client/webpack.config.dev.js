const { merge } = require('webpack-merge');
const common = require('./webpack.common.js');

const API_TARGET = process.env.API_TARGET ?? 'http://127.0.0.1:8080';

module.exports = merge(common, {
  mode: 'development',
  devtool: 'inline-source-map',
  devServer: {
    liveReload: true,
    hot: true,
    open: true,
    static: ['./'],
    historyApiFallback: true,
    // SSE 응답이 버퍼링되지 않도록 압축 없이 그대로 통과시킨다
    compress: false,
    proxy: [
      {
        context: ['/api'],
        target: API_TARGET,
      },
    ],
  },
});
