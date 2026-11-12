# Third-party components

Dropper는 FFmpeg/ffprobe와 코덱 라이브러리를 동봉해 로컬에서 영상을 처리합니다. 기본 x264/x265 구성을 포함한 FFmpeg 바이너리는 GPL 조건을 따릅니다. 각 외부 구성 요소는 원래 라이선스를 유지합니다. 앱 자체 소스의 공개·배포 라이선스는 아직 정하지 않았습니다.

- FFmpeg: https://ffmpeg.org/ · https://ffmpeg.org/legal.html · https://ffmpeg.org/download.html
- x264: https://www.videolan.org/developers/x264.html
- x265: https://www.videolan.org/developers/x265.html
- SVT-AV1: https://gitlab.com/AOMediaCodec/SVT-AV1
- Tauri: https://github.com/tauri-apps/tauri (MIT/Apache-2.0)
- React: https://github.com/facebook/react (MIT)
- Lucide: https://lucide.dev/license (ISC)
- Intel macOS 정적 엔진 제공자: https://evermeet.cx/ffmpeg/ (FFmpeg 공식 다운로드 페이지의 링크). 버전·SHA-256은 `scripts/mac-intel-media.json`에 기록한다.

`scripts/bundle-media.py`는 설치된 엔진과 비시스템 동적 라이브러리의 복사본만 변경합니다. 생성 폴더의 `manifest.json`에 SHA-256과 로컬 출처가, `licenses/`에 발견된 라이선스·엔진 빌드 정보가 포함됩니다. 이는 외부 배포용 완전한 대응 소스 패키지를 대신하지 않습니다.

바이너리 배포 시에는 실제 포함한 각 라이브러리의 라이선스 고지와 해당 버전의 대응 소스·빌드 스크립트 제공 조건을 확인해야 합니다. 설치 파일은 GitHub Releases에서 제공하며 패키지 크기·최소 OS 버전은 실제 동봉한 엔진의 구성에 따라 달라집니다.
