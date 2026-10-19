$ErrorActionPreference = 'Stop'
if ($PSVersionTable.PSEdition -eq 'Core') {
    & "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -File $PSCommandPath
    if ($LASTEXITCODE -ne 0) { throw 'Icon generation failed.' }
    return
}
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
public static class MleIconBuilder {
    static GraphicsPath Round(float x, float y, float w, float h, float r) {
        var p = new GraphicsPath();
        p.AddArc(x, y, r*2, r*2, 180, 90);
        p.AddArc(x+w-r*2, y, r*2, r*2, 270, 90);
        p.AddArc(x+w-r*2, y+h-r*2, r*2, r*2, 0, 90);
        p.AddArc(x, y+h-r*2, r*2, r*2, 90, 90);
        p.CloseFigure();
        return p;
    }
    static void Draw(Graphics g) {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        using (var purple = new SolidBrush(Color.FromArgb(91,83,219)))
        using (var lilac = new SolidBrush(Color.FromArgb(183,177,255)))
        using (var white = new SolidBrush(Color.White))
        using (var tile = Round(8,8,240,240,56))
        using (var back = Round(112,44,104,120,22))
        using (var front = Round(38,88,116,104,22))
        using (var darkPen = new Pen(Color.FromArgb(64,54,174),8))
        using (var accentPen = new Pen(Color.FromArgb(91,83,219),9)) {
            darkPen.StartCap = darkPen.EndCap = LineCap.Round;
            accentPen.StartCap = accentPen.EndCap = LineCap.Round;
            accentPen.LineJoin = LineJoin.Round;
            g.FillPath(purple,tile);
            g.FillPath(lilac,back);
            g.DrawLine(darkPen,143,76,191,76);
            g.DrawLine(darkPen,167,65,174,76);
            g.DrawBezier(darkPen,149,89,154,113,171,131,193,140);
            g.DrawBezier(darkPen,183,89,178,113,162,131,140,140);
            g.FillPath(white,front);
            g.FillPolygon(white,new PointF[]{new PointF(60,182),new PointF(60,210),new PointF(94,182)});
            g.DrawLines(accentPen,new PointF[]{new PointF(68,164),new PointF(92,112),new PointF(100,112),new PointF(124,164)});
            g.DrawLine(accentPen,78,149,115,149);
        }
    }
    public static void Build(string directory) {
        var sizes = new[]{16,20,24,32,40,48,64,128,256};
        var images = new byte[sizes.Length][];
        using (var master = new Bitmap(1024,1024,PixelFormat.Format32bppArgb)) {
            using (var g = Graphics.FromImage(master)) { g.ScaleTransform(4,4); Draw(g); }
            for(int i=0;i<sizes.Length;i++) {
                using(var small = new Bitmap(sizes[i],sizes[i],PixelFormat.Format32bppArgb))
                using(var g = Graphics.FromImage(small))
                using(var stream = new MemoryStream()) {
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.PixelOffsetMode = PixelOffsetMode.HighQuality;
                    g.DrawImage(master,new Rectangle(0,0,sizes[i],sizes[i]));
                    small.Save(stream,ImageFormat.Png);
                    images[i] = stream.ToArray();
                    if(sizes[i]==256) small.Save(Path.Combine(directory,"mle.png"),ImageFormat.Png);
                }
            }
        }
        using(var writer = new BinaryWriter(File.Create(Path.Combine(directory,"mle.ico")))) {
            writer.Write((ushort)0); writer.Write((ushort)1); writer.Write((ushort)sizes.Length);
            int offset = 6 + 16*sizes.Length;
            for(int i=0;i<sizes.Length;i++) {
                writer.Write((byte)(sizes[i]==256?0:sizes[i]));
                writer.Write((byte)(sizes[i]==256?0:sizes[i]));
                writer.Write((byte)0); writer.Write((byte)0);
                writer.Write((ushort)1); writer.Write((ushort)32);
                writer.Write(images[i].Length); writer.Write(offset); offset+=images[i].Length;
            }
            foreach(var image in images) writer.Write(image);
        }
    }
}
'@
[MleIconBuilder]::Build((Join-Path (Split-Path $PSScriptRoot -Parent) 'assets'))
Write-Output 'Generated assets\mle.ico (16-256 px) and assets\mle.png'
