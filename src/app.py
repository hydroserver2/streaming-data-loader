import os
import json
import sys
import pystray
import scheduler
import logging
import setup
import webbrowser
from PIL import Image
from appdirs import user_data_dir
from logging.handlers import RotatingFileHandler


class HydroLoaderApp:

    def __init__(self, service_url, setup_window):
        base_path = getattr(sys, '_MEIPASS', 'assets')

        image = Image.open(os.path.join(base_path, 'tray_icon.png'))

        menu = (
            pystray.MenuItem('HydroLoader is running', lambda: None, enabled=False),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem('Data Sources Dashboard', self.open_data_sources_dashboard),
            pystray.MenuItem('HydroLoader Settings', lambda: None),
            pystray.MenuItem('HydroLoader Logs', lambda: None),
            pystray.MenuItem('Quit HydroLoader', self.close_app)
        )

        self.tray = pystray.Icon('hydroloader', image, 'HydroServer Data Loader', menu)
        self.setup = setup_window
        self.app_dir = user_data_dir('HydroLoader', 'CIROH')

        if not os.path.exists(self.app_dir):
            os.makedirs(self.app_dir)

        self.hydroloader_instance = None
        self.hydroserver_username = None
        self.hydroserver_password = None
        self.hydroserver_url = service_url

    def open_data_sources_dashboard(self):
        webbrowser.open(f'{self.hydroserver_url}/data-sources')

    def get_settings(self):
        settings_path = os.path.join(self.app_dir, 'settings.json')
        if os.path.exists(settings_path):
            with open(settings_path, 'r') as settings_file:
                settings = json.loads(settings_file.read() or 'null') or {}
                self.hydroloader_instance = settings.get('instance')
                self.hydroserver_username = settings.get('username')
                self.hydroserver_password = settings.get('password')

    def launch_background(self):
        self.get_settings()

        if all([
            self.hydroloader_instance, self.hydroserver_username, self.hydroserver_password
        ]):
            self.setup.withdraw()
            scheduler.HydroLoaderScheduler(
                service=self.hydroserver_url,
                instance=self.hydroloader_instance,
                auth=(
                    self.hydroserver_username,
                    self.hydroserver_password
                )
            )
            self.tray.run_detached()

    def launch_app(self):
        self.setup.after(0, self.launch_background)
        self.setup.mainloop()

    def close_app(self):
        self.setup.destroy()


if __name__ == '__main__':

    hydroserver_url = 'http://hydroserver-dev.ciroh.org'

    hydroloader_logger = logging.getLogger('hydroloader')
    scheduler_logger = logging.getLogger('scheduler')

    stream_handler = logging.StreamHandler()
    hydroloader_logger.addHandler(stream_handler)
    scheduler_logger.addHandler(stream_handler)

    user_dir = user_data_dir('HydroLoader', 'CIROH')

    if not os.path.exists(user_dir):
        os.makedirs(user_dir)

    log_path = os.path.join(user_dir, 'hydroloader.log')

    log_handler = RotatingFileHandler(
        filename=log_path,
        mode='a',
        maxBytes=20 * 1024 * 1024,
        backupCount=3
    )
    hydroloader_logger.addHandler(log_handler)
    scheduler_logger.addHandler(log_handler)

    logging.basicConfig(
        format='%(asctime)s %(levelname)-8s %(message)s',
        level=logging.INFO,
        datefmt='%Y-%m-%d %H:%M:%S',
        force=True,
        handlers=[
            log_handler, stream_handler
        ]
    )

    hydroloader_setup = setup.AppSetup(service_url=hydroserver_url)
    hydroloader = HydroLoaderApp(service_url=hydroserver_url, setup_window=hydroloader_setup)
    hydroloader_setup.callback = hydroloader.launch_background
    hydroloader.launch_app()
