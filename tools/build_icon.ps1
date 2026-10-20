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
        using (var background = new SolidBrush(Color.FromArgb(16,16,16)))
        using (var blue = new SolidBrush(Color.FromArgb(158,207,255)))
        using (var orange = new SolidBrush(Color.FromArgb(243,162,116)))
        using (var tile = Round(8,8,240,240,54))
        using (var top = new GraphicsPath())
        using (var bottom = new GraphicsPath())
        using (var bluePen = new Pen(blue,21))
        using (var orangePen = new Pen(orange,21)) {
            bluePen.StartCap = bluePen.EndCap = LineCap.Round;
            orangePen.StartCap = orangePen.EndCap = LineCap.Round;
            bluePen.LineJoin = orangePen.LineJoin = LineJoin.Round;
            g.FillPath(background,tile);
            top.AddLine(62,162,62,111);
            top.AddBezier(62,111,62,88,72,79,94,79);
            top.AddLine(94,79,148,79);
            g.DrawPath(bluePen,top);
            g.FillPolygon(blue,new PointF[]{new PointF(169,79),new PointF(138.98f,64.18f),new PointF(138.98f,93.82f)});
            bottom.AddLine(194,94,194,145);
            bottom.AddBezier(194,145,194,168,184,177,162,177);
            bottom.AddLine(162,177,108,177);
            g.DrawPath(orangePen,bottom);
            g.FillPolygon(orange,new PointF[]{new PointF(87,177),new PointF(117.02f,162.18f),new PointF(117.02f,191.82f)});
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
