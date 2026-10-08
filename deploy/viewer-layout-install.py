from pathlib import Path
import shutil,os,sys
stage=Path(sys.argv[1]);dist=Path('/home/radxa/microduck-observer/current/frontend/dist');backup=stage/'index.before.html';shutil.copy2(dist/'index.html',backup)
for file in (stage/'assets').iterdir():shutil.copy2(file,dist/'assets'/file.name)
shutil.copy2(stage/'index.html',dist/'index.layout-next');os.replace(dist/'index.layout-next',dist/'index.html')
print('Frontend installed; control service was not restarted')
