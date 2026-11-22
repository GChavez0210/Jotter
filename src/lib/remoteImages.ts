/** Filters image sources in an inert template before the HTML reaches a live document. */
export function blockRemoteImages(html: string): string {
  if (!/<(?:img|image|feimage|source)[\s/>]/i.test(html)) return html;
  const template = document.createElement('template');
  template.innerHTML = html;
  const isRemote = (source: string) => {
    const normalized = source.replace(/[\t\r\n]/g, '').trim();
    return /^(?:https?:|[/\\]{2})/i.test(normalized);
  };

  for (const image of template.content.querySelectorAll('img, image, feImage, picture source')) {
    image.removeAttribute('srcset');
    if (!['src', 'href', 'xlink:href'].some((attr) => isRemote(image.getAttribute(attr) ?? ''))) continue;
    if (image.localName === 'img') {
      const placeholder = document.createElement('span');
      placeholder.className = 'jotter-remote-image-blocked';
      placeholder.textContent = image.getAttribute('alt') || '🖼';
      image.replaceWith(placeholder);
    } else {
      image.remove();
    }
  }
  return template.innerHTML;
}
