// Host reason (ARC-03): VS Code's Electron test host loads this CommonJS entry.
const driver = process.env.RICHVIEW_VSCODE_DRIVER;
if (!driver) throw new Error('Use the isolated VS Code test runner');
module.exports = require(driver);
