// Host reason (ARC-03): choose browser-gate expectations for the selected local site build.
const suffix = ' Then make me a beautiful report from my data, or from the starter example if I have none. Check it, fix anything it flags, and show me the result. Keep everything on my machine.';
const repository = 'https://github.com/1vecera/richview';

export function siteModeExpectations(installEnabled, repositoryPublic) {
  if (installEnabled && !repositoryPublic) throw new Error('An enabled install release requires a public repository.');
  const prompt = (installEnabled
    ? `Install RichView from ${repository}/releases/latest and run richview learn.`
    : 'If RichView is installed, run richview learn.') + suffix;
  const links = repositoryPublic ? ['Docs', 'Gallery', 'RichView on GitHub', 'Sign in'] : ['Docs', 'Gallery', 'Sign in'];
  return { prompt, header: { links, github: repositoryPublic ? repository : null, motto: 0 } };
}
