/**
 * Preprocessing pipeline for photos and scans:
 * 1. Upscales up to 4x (maintaining reasonable bounds for Gemini multimodal input)
 * 2. Contrast-stretching (normalizes dynamic range between min and max luminance)
 * 3. Sharpening filter (3x3 Laplacian / unsharp kernel to enhance text legibility)
 */

export interface PreprocessedImage {
  mimeType: string;
  data: string; // Base64 without data: prefix
  dataUrl: string; // Full data URL
  width: number;
  height: number;
  upscaleFactor: number;
}

export async function preprocessInvoiceImage(file: File | Blob): Promise<PreprocessedImage> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const img = new Image();

    img.onload = () => {
      URL.revokeObjectURL(url);
      try {
        const origW = img.naturalWidth || img.width;
        const origH = img.naturalHeight || img.height;

        // Upscale up to 4x, bounded to 2048px maximum dimension to avoid excessive memory
        let upscaleFactor = 1;
        const maxDim = Math.max(origW, origH);
        if (maxDim < 600) {
          upscaleFactor = 4;
        } else if (maxDim < 1000) {
          upscaleFactor = 2;
        } else if (maxDim < 1600) {
          upscaleFactor = 1.5;
        } else {
          upscaleFactor = 1;
        }

        const targetW = Math.round(origW * upscaleFactor);
        const targetH = Math.round(origH * upscaleFactor);

        const canvas = document.createElement('canvas');
        canvas.width = targetW;
        canvas.height = targetH;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          throw new Error('Failed to create canvas 2D context');
        }

        ctx.imageSmoothingEnabled = true;
        ctx.imageSmoothingQuality = 'high';
        ctx.drawImage(img, 0, 0, targetW, targetH);

        const imgData = ctx.getImageData(0, 0, targetW, targetH);
        const pixels = imgData.data;

        // 1. Contrast Stretching (Histogram stretching)
        let minLum = 255;
        let maxLum = 0;
        for (let i = 0; i < pixels.length; i += 4) {
          const lum = 0.299 * pixels[i] + 0.587 * pixels[i + 1] + 0.114 * pixels[i + 2];
          if (lum < minLum) minLum = lum;
          if (lum > maxLum) maxLum = lum;
        }

        if (maxLum - minLum > 10) {
          const range = maxLum - minLum;
          for (let i = 0; i < pixels.length; i += 4) {
            pixels[i] = Math.min(255, Math.max(0, ((pixels[i] - minLum) / range) * 255));
            pixels[i + 1] = Math.min(255, Math.max(0, ((pixels[i + 1] - minLum) / range) * 255));
            pixels[i + 2] = Math.min(255, Math.max(0, ((pixels[i + 2] - minLum) / range) * 255));
          }
        }

        // 2. 3x3 Sharpen Convolution Filter
        // Kernel:
        // [ 0, -1,  0]
        // [-1,  5, -1]
        // [ 0, -1,  0]
        const sharpenCanvas = document.createElement('canvas');
        sharpenCanvas.width = targetW;
        sharpenCanvas.height = targetH;
        const sharpenCtx = sharpenCanvas.getContext('2d');
        if (!sharpenCtx) {
          throw new Error('Failed to create sharpen canvas context');
        }

        const outputData = sharpenCtx.createImageData(targetW, targetH);
        const src = pixels;
        const dst = outputData.data;

        const w = targetW;
        const h = targetH;

        for (let y = 1; y < h - 1; y++) {
          for (let x = 1; x < w - 1; x++) {
            const idx = (y * w + x) * 4;
            const top = ((y - 1) * w + x) * 4;
            const bottom = ((y + 1) * w + x) * 4;
            const left = (y * w + (x - 1)) * 4;
            const right = (y * w + (x + 1)) * 4;

            for (let c = 0; c < 3; c++) {
              const val =
                5 * src[idx + c] -
                (src[top + c] + src[bottom + c] + src[left + c] + src[right + c]);
              dst[idx + c] = Math.min(255, Math.max(0, val));
            }
            dst[idx + 3] = src[idx + 3]; // Alpha
          }
        }

        sharpenCtx.putImageData(outputData, 0, 0);

        const dataUrl = sharpenCanvas.toDataURL('image/jpeg', 0.94);
        const data = dataUrl.replace(/^data:image\/\w+;base64,/, '');

        resolve({
          mimeType: 'image/jpeg',
          data,
          dataUrl,
          width: targetW,
          height: targetH,
          upscaleFactor,
        });
      } catch (err) {
        reject(err);
      }
    };

    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error('Failed to load image for preprocessing'));
    };

    img.src = url;
  });
}

/**
 * Loads PDF.js from CDN dynamically if not already loaded on window
 */
export async function loadPdfJs(): Promise<any> {
  const win = window as any;
  if (win.pdfjsLib) {
    return win.pdfjsLib;
  }

  return new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = 'https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.min.js';
    script.onload = () => {
      const pdfjs = win.pdfjsLib;
      if (pdfjs) {
        pdfjs.GlobalWorkerOptions.workerSrc =
          'https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.worker.min.js';
        resolve(pdfjs);
      } else {
        reject(new Error('PDF.js library failed to initialize'));
      }
    };
    script.onerror = () => reject(new Error('Failed to load PDF.js from CDN'));
    document.head.appendChild(script);
  });
}

/**
 * Renders up to 5 pages of a scanned PDF into preprocessed page images
 */
export async function renderScannedPdfPages(pdfBytes: Uint8Array, maxPages = 5): Promise<PreprocessedImage[]> {
  const pdfjs = await loadPdfJs();
  const loadingTask = pdfjs.getDocument({ data: pdfBytes });
  const pdf = await loadingTask.promise;

  const pagesToRender = Math.min(pdf.numPages, maxPages);
  const images: PreprocessedImage[] = [];

  for (let pageNum = 1; pageNum <= pagesToRender; pageNum++) {
    const page = await pdf.getPage(pageNum);
    const viewport = page.getViewport({ scale: 2.0 }); // 2x scale for high OCR accuracy

    const canvas = document.createElement('canvas');
    canvas.width = viewport.width;
    canvas.height = viewport.height;
    const ctx = canvas.getContext('2d');
    if (!ctx) continue;

    await page.render({ canvasContext: ctx, viewport }).promise;

    // Convert canvas to blob and run contrast-stretching and sharpening
    const blob: Blob = await new Promise((res) => canvas.toBlob((b) => res(b!), 'image/jpeg', 0.95));
    const processed = await preprocessInvoiceImage(blob);
    images.push(processed);
  }

  return images;
}
